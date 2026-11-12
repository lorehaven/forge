//! purge: delete the audio files of blacklisted albums.
//!
//! Blacklisting an album takes it (and the songs only it held) out of the index but leaves their files in the audio folder, and
//! the index no longer knows their names. So this asks Deezer again for each blacklisted album's tracklist, rebuilds the file
//! names the way sync names them, and deletes the files of those names that are in the folder, unless a song still in the index
//! has that name (the song is also on an album that is kept). Anything else in the folder, such as a file put there by hand, is
//! never touched. A preview (`dry_run`) lists the files and their size and deletes nothing.

use crate::app::App;
use crate::files;
use crate::jobs::Log;
use crate::sync::contributors;
use songbook_core::naming::{album_key, make_filename, norm};
use std::collections::{BTreeSet, HashMap, HashSet};

/// Lists (and, unless `dry_run`, deletes) the files of every blacklisted album.
pub async fn run(app: &App, dry_run: bool, log: &Log) -> Result<(), String> {
    let (blacklisted, follows, kept) = app
        .index
        .run(async |db| {
            let rows: Vec<(String, String, String)> = db
                .query(
                    "SELECT artist, album_key, title FROM blacklist ORDER BY artist COLLATE \"C\", title COLLATE \"C\"",
                    &[],
                )
                .await?
                .rows
                .iter()
                .map(|r| {
                    (
                        crate::index::str_at(r, 0),
                        crate::index::str_at(r, 1),
                        crate::index::str_at(r, 2),
                    )
                })
                .collect();
            let kept: HashSet<String> = db
                .query("SELECT filename FROM songs", &[])
                .await?
                .rows
                .iter()
                .map(|r| crate::index::str_at(r, 0))
                .collect();
            Ok((rows, crate::index::follow::follows(db).await?, kept))
        })
        .await
        .map_err(|e| e.to_string())?;
    if blacklisted.is_empty() {
        log.line("Nothing is blacklisted.");
        return Ok(());
    }
    let on_disk = files::file_index(&app.config.data);
    log.line(format!(
        "{} blacklisted albums; {} files in {}",
        blacklisted.len(),
        on_disk.len(),
        app.config.data.display()
    ));
    let ids: HashMap<String, i64> = follows
        .iter()
        .filter_map(|f| Some((norm(&f.name), f.deezer_id.parse().ok()?)))
        .collect();
    let mut albums: HashMap<i64, Vec<crate::deezer::AlbumRef>> = HashMap::new();
    let mut victims: BTreeSet<String> = BTreeSet::new(); // file names
    let mut stems_seen: HashSet<String> = HashSet::new();
    for (artist, key, title) in &blacklisted {
        let Some(id) = ids.get(&norm(artist)).copied() else {
            log.line(format!(
                "  skip {artist} - {title}: the artist is not followed or not synced"
            ));
            continue;
        };
        if let std::collections::hash_map::Entry::Vacant(e) = albums.entry(id) {
            e.insert(app.deezer.artist_albums(id).await?);
        }
        let editions: Vec<_> = albums[&id]
            .iter()
            .filter(|a| &album_key(&a.title) == key)
            .collect();
        if editions.is_empty() {
            log.line(format!("  {artist} - {title}: Deezer no longer lists it"));
        }
        for edition in editions {
            let album = app.deezer.album(edition.id).await?;
            for track in &album.tracks {
                let detail = app.deezer.track(track.id).await?;
                let people = contributors(track, detail.as_ref());
                let names: Vec<&str> = people.iter().map(|(_, n)| n.as_str()).collect();
                let song = detail
                    .as_ref()
                    .map(|d| d.title.clone())
                    .filter(|t| !t.is_empty())
                    .unwrap_or_else(|| track.title.clone());
                let stem = make_filename(&names, &song);
                if !stems_seen.insert(stem.clone()) {
                    continue;
                }
                if kept.contains(&stem) {
                    log.line(format!("  keep {stem}: a song of the library"));
                } else if let Some(found) = on_disk.get(&stem) {
                    victims.extend(found.iter().cloned());
                }
            }
        }
    }
    let (mut count, mut bytes, mut failed) = (0u32, 0u64, 0u32);
    for name in &victims {
        let path = app.config.data.join(name);
        let size = std::fs::metadata(&path).map_or(0, |m| m.len());
        if dry_run {
            log.line(format!("  would delete {name} ({} KiB)", size / 1024));
            count += 1;
            bytes += size;
        } else if std::fs::remove_file(&path).is_ok() {
            log.line(format!("  deleted {name} ({} KiB)", size / 1024));
            count += 1;
            bytes += size;
        } else {
            log.line(format!("  ERROR: cannot delete {name}"));
            failed += 1;
        }
    }
    log.line(format!(
        "{} {count} files, {} MiB{}",
        if dry_run { "would delete" } else { "deleted" },
        bytes / (1024 * 1024),
        if failed > 0 {
            format!("; {failed} failed")
        } else {
            String::new()
        }
    ));
    if dry_run && count > 0 {
        log.line("Nothing was deleted. Run \"Delete blacklisted files\" to delete these.");
    }
    if failed > 0 {
        Err(format!("{failed} files could not be deleted"))
    } else {
        Ok(())
    }
}
