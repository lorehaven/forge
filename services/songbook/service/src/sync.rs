//! sync: fetch the discographies of the followed artists from Deezer into the index; tags: rebuild the derived tags.

use crate::app::App;
use crate::deezer::{AlbumDetail, TrackDetail, TrackRef};
use crate::index::{follow, songs};
use crate::jobs::Log;
use songbook_core::naming::{album_key, norm};
use std::collections::{HashMap, HashSet};

/// A track of a fetched album: the listing entry, Deezer's detail if it had any, and the artists to credit.
type StagedTrack = (TrackRef, Option<TrackDetail>, Vec<(i64, String)>);

/// One album fetched, waiting to be written.
struct Staged {
    owner: i64,
    album: AlbumDetail,
    tracks: Vec<StagedTrack>,
}

/// The parts of a combined credit such as `Nate Dogg featuring Snoop Dogg` or `2Pac, Nate Dogg`: split at commas, `&`, `/`,
/// `feat.`/`featuring`/`ft.` (followed by a space), `and` and `with`.
#[must_use]
pub fn credit_parts(name: &str) -> Vec<String> {
    let chars: Vec<char> = name.chars().collect();
    let lower: Vec<char> = name.to_lowercase().chars().collect();
    let boundary = |i: usize| i == 0 || !(chars[i - 1].is_alphanumeric() || chars[i - 1] == '_');
    let word_end = |i: usize| i >= chars.len() || !(chars[i].is_alphanumeric() || chars[i] == '_');
    let starts = |i: usize, w: &str| {
        lower.len() >= i + w.chars().count()
            && lower[i..].iter().zip(w.chars()).all(|(a, b)| *a == b)
    };
    let space_at = |i: usize| i < chars.len() && chars[i].is_whitespace();
    let mut parts = Vec::new();
    let mut cur = String::new();
    let mut i = 0;
    while i < chars.len() {
        let skip = if matches!(chars[i], ',' | '&' | '/') {
            Some(1)
        } else if boundary(i) && starts(i, "featuring") && space_at(i + 9) {
            Some(9)
        } else if boundary(i) && starts(i, "feat.") && space_at(i + 5) {
            Some(5)
        } else if boundary(i) && starts(i, "feat") && space_at(i + 4) {
            Some(4)
        } else if boundary(i) && starts(i, "ft.") && space_at(i + 3) {
            Some(3)
        } else if boundary(i) && starts(i, "ft") && space_at(i + 2) {
            Some(2)
        } else if boundary(i) && starts(i, "and") && word_end(i + 3) {
            Some(3)
        } else if boundary(i) && starts(i, "with") && word_end(i + 4) {
            Some(4)
        } else {
            None
        };
        if let Some(n) = skip {
            parts.push(std::mem::take(&mut cur));
            i += n;
        } else {
            cur.push(chars[i]);
            i += 1;
        }
    }
    parts.push(cur);
    parts
        .into_iter()
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect()
}

/// Deezer sometimes lists a compilation "artist" such as `Nate Dogg featuring Snoop Dogg`. When every part of such a name is
/// an artist known from elsewhere, it is replaced by those artists, so a song and its file name come out the same whichever
/// release it was found on. Names with an unknown part (`Earth, Wind & Fire`) are left alone.
#[must_use]
pub fn split_combined(
    people: &[(i64, String)],
    known: &HashMap<String, (i64, String)>,
) -> Vec<(i64, String)> {
    let mut out: Vec<(i64, String)> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for (id, name) in people {
        let parts = credit_parts(name);
        let group: Vec<(i64, String)> =
            if parts.len() > 1 && parts.iter().all(|p| known.contains_key(&norm(p))) {
                parts.iter().map(|p| known[&norm(p)].clone()).collect()
            } else {
                vec![(*id, name.clone())]
            };
        for (gid, gname) in group {
            if seen.insert(norm(&gname)) {
                out.push((gid, gname));
            }
        }
    }
    out
}

/// An artist name as Deezer sends it, without the invisible direction marks some names carry, and without the mis-decoded
/// form of the left-to-right mark (`â€Ž`) that turns up in front of a few names.
#[must_use]
pub fn clean_name(name: &str) -> String {
    name.replace("\u{e2}\u{20ac}\u{17d}", "")
        .chars()
        .filter(|c| !matches!(c, '\u{200e}' | '\u{200f}' | '\u{200b}' | '\u{feff}'))
        .collect::<String>()
        .trim()
        .to_string()
}

/// A track's artists: Deezer's contributor list, else the artist the album tracklist names.
#[must_use]
pub fn contributors(track: &TrackRef, detail: Option<&TrackDetail>) -> Vec<(i64, String)> {
    let people = detail.map(|d| d.contributors.clone()).unwrap_or_default();
    let people = if people.is_empty() {
        track.artist.iter().cloned().collect()
    } else {
        people
    };
    people
        .into_iter()
        .map(|(id, name)| (id, clean_name(&name)))
        .filter(|(_, name)| !name.is_empty())
        .collect()
}

pub async fn run(app: &App, singles: bool, log: &Log) -> Result<(), String> {
    run_with(app, singles, false, log).await
}

/// [`run`]; with `fresh_ratings` the tracks are asked of Deezer again (saved ones older than 12 hours), so their ratings are
/// current. That is one request per track, spread out by the client's throttle.
pub async fn run_with(
    app: &App,
    singles: bool,
    fresh_ratings: bool,
    log: &Log,
) -> Result<(), String> {
    let deezer = if fresh_ratings {
        app.deezer
            .with_track_ttl(std::time::Duration::from_hours(12))
    } else {
        app.deezer.clone()
    };
    let follows = app
        .index
        .run(follow::follows)
        .await
        .map_err(|e| e.to_string())?;
    let banned = app
        .index
        .run(follow::blacklist)
        .await
        .map_err(|e| e.to_string())?;
    let mut matches: Vec<(String, String, String, String, String)> = Vec::new();
    let (mut resolved, mut touched, mut staged): (HashSet<i64>, HashSet<i64>, Vec<Staged>) =
        (HashSet::new(), HashSet::new(), Vec::new());
    for f in &follows {
        let Some(hit) = app.deezer.resolve(&f.name, &f.pin).await? else {
            log.line(format!("NO MATCH: {}", f.name));
            matches.push((
                f.name.clone(),
                String::new(),
                String::new(),
                String::new(),
                "NO MATCH".into(),
            ));
            continue;
        };
        matches.push((
            f.name.clone(),
            hit.name.clone(),
            hit.id.to_string(),
            hit.fans.to_string(),
            String::new(),
        ));
        let (name, hit_name, hit_id) = (f.name.clone(), hit.name.clone(), hit.id);
        let owner = app
            .index
            .run(move |db| songs::upsert_artist(db, hit_id, &hit_name, Some(&name)))
            .await
            .map_err(|e| e.to_string())?;
        resolved.insert(owner);
        let mut seen = HashSet::new();
        let mut albums = Vec::new();
        for al in app.deezer.artist_albums(hit.id).await? {
            let wanted = al.record_type == "album"
                || al.record_type == "ep"
                || (singles && al.record_type == "single");
            let key = album_key(&al.title);
            if !wanted || banned.contains(&(norm(&f.name), key.clone())) || !seen.insert(key) {
                continue;
            }
            albums.push(al);
        }
        log.line(format!(
            "{} -> {} ({} fans), {} releases",
            f.name,
            hit.name,
            hit.fans,
            albums.len()
        ));
        for al in albums {
            let album = app.deezer.album(al.id).await?;
            if !touched.insert(album.info.deezer_id) {
                continue; // a collaboration album already reached through another followed artist
            }
            let fetches: Vec<_> = album
                .tracks
                .iter()
                .map(|t| {
                    let (deezer, id) = (deezer.clone(), t.id);
                    tokio::spawn(async move { deezer.track(id).await })
                })
                .collect();
            let mut tracks = Vec::new();
            for (t, fetch) in album.tracks.iter().zip(fetches) {
                let detail = fetch.await.map_err(|e| e.to_string())??;
                let people = contributors(t, detail.as_ref());
                tracks.push((t.clone(), detail, people));
            }
            staged.push(Staged {
                owner,
                album,
                tracks,
            });
        }
    }

    let names: HashSet<String> = follows.iter().map(|f| f.name.clone()).collect();
    let (gone_songs, gone_artists, songs_total, entries) = app
        .index
        .run(move |db| {
            db.transaction(|db| {
                // The vocabulary for splitting combined credits: every plain artist in the fetched data and every followed one.
                let mut known: HashMap<String, (i64, String)> = db
                    .query("SELECT deezer_id, name FROM artists WHERE listed_as IS NOT NULL AND deezer_id IS NOT NULL", &[])?
                    .rows
                    .iter()
                    .map(|r| (norm(&crate::index::str_at(r, 1)), (crate::index::int_at(r, 0), crate::index::str_at(r, 1))))
                    .collect();
                for s in &staged {
                    for (_, _, people) in &s.tracks {
                        for (id, name) in people {
                            if credit_parts(name).len() <= 1 {
                                known.entry(norm(name)).or_insert_with(|| (*id, name.clone()));
                            }
                        }
                    }
                }
                for s in &staged {
                    let album_id = songs::upsert_album(db, &s.album.info, s.owner)?;
                    db.execute("DELETE FROM song_album WHERE album_id = ?", &[crate::index::int(album_id)])?;
                    for (i, (t, det, people)) in s.tracks.iter().enumerate() {
                        let people = split_combined(people, &known);
                        if people.is_empty() {
                            continue;
                        }
                        let title = det.as_ref().map(|d| d.title.clone()).filter(|t| !t.is_empty()).unwrap_or_else(|| t.title.clone());
                        let duration = det.as_ref().and_then(|d| d.duration).or(t.duration);
                        let song = songs::upsert_song(db, &people, &title, duration, t.id)?;
                        songs::link_album(db, song, album_id, i as i64 + 1)?;
                        if let Some(rank) = det.as_ref().and_then(|d| d.rank).filter(|r| *r > 0) {
                            // A song on several releases keeps the best rank of its tracks.
                            db.execute(
                                "UPDATE songs SET rank = ? WHERE id = ? AND (rank IS NULL OR rank < ?)",
                                &[crate::index::int(rank), crate::index::int(song), crate::index::int(rank)],
                            )?;
                        }
                    }
                }
                // Artists removed from the follow list: forget their albums. Artists with no match keep what they had.
                for r in db.query("SELECT id, listed_as FROM artists WHERE listed_as IS NOT NULL", &[])?.rows {
                    let (id, listed) = (crate::index::int_at(&r, 0), crate::index::str_at(&r, 1));
                    if !names.contains(&listed) {
                        for al in db.query("SELECT id FROM albums WHERE owner_id = ?", &[crate::index::int(id)])?.rows {
                            songs::drop_album(db, crate::index::int_at(&al, 0))?;
                        }
                        db.execute("UPDATE artists SET listed_as = NULL WHERE id = ?", &[crate::index::int(id)])?;
                    }
                }
                // Albums of a resolved artist that did not turn up this time (blacklisted, merged editions, withdrawn).
                for owner in &resolved {
                    for al in db.query("SELECT id, deezer_id FROM albums WHERE owner_id = ?", &[crate::index::int(*owner)])?.rows {
                        if !touched.contains(&crate::index::int_at(&al, 1)) {
                            songs::drop_album(db, crate::index::int_at(&al, 0))?;
                        }
                    }
                }
                let (gone_songs, gone_artists) = songs::prune(db)?;
                let cmap = follow::category_map(db)?;
                songs::retag(db, &cmap)?;
                for (name, deezer_name, id, fans, note) in &matches {
                    follow::set_match(db, name, deezer_name, id, fans, note)?;
                }
                let total = crate::index::int_at(&db.query("SELECT COUNT(*) FROM songs", &[])?.rows[0], 0);
                let entries = crate::index::int_at(&db.query("SELECT COUNT(*) FROM song_album", &[])?.rows[0], 0);
                Ok((gone_songs, gone_artists, total, entries))
            })
        })
        .await
        .map_err(|e| e.to_string())?;
    log.line(format!(
        "index: {songs_total} songs ({entries} album entries); dropped {gone_songs} songs and {gone_artists} artists no longer on any album"
    ));
    Ok(())
}

/// Rebuilds the category and genre tags without fetching.
pub async fn tags(app: &App, log: &Log) -> Result<(), String> {
    let n = app
        .index
        .run(|db| {
            let cmap = follow::category_map(db)?;
            db.transaction(|db| songs::retag(db, &cmap))
        })
        .await
        .map_err(|e| e.to_string())?;
    log.line(format!("rebuilt {n} song-tag links"));
    Ok(())
}
