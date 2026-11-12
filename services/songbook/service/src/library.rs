//! The library as a listener sees it: the songs that have a file, with who is on them, the album they are found on and
//! their category. Built from the index and a look at the audio folder, kept for a short while and rebuilt when either
//! changes (a download adds a file without the index knowing).

use crate::files::file_index;
use crate::index::{Index, Result, int_at, opt_int_at, str_at};
use songbook_core::{
    AlbumInfo, AlbumQuery, ArtistInfo, CategoryInfo, Home, Mix, Overview, Search, SongIds,
    SongInfo, SongList, SongQuery, popularity,
};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

/// How long a built catalog is used without a look at the audio folder again.
const FRESH: Duration = Duration::from_secs(30);

/// How many songs / albums a list shows at most when the query does not say.
pub const SONGS: usize = 500;
pub const ALBUMS: usize = 500;
/// How many ids a queue holds at most.
pub const IDS: usize = 20_000;

#[derive(Debug)]
struct Song {
    info: SongInfo,
    /// The file in the audio folder, with its extension.
    file: String,
    track: i64,
    /// How popular, 0-100: Deezer's rank and the views of its video when the index has them, else how well known its
    /// artists are and how many albums it is on.
    score: u32,
    categories: Vec<String>,
    /// Lowercase title, album and artists: what search looks in.
    haystack: String,
}

#[derive(Debug)]
struct Album {
    info: AlbumInfo,
    /// Playable songs on it, in track order (indexes into `Catalog::songs`).
    songs: Vec<usize>,
    categories: HashSet<String>,
}

#[derive(Debug, Default)]
pub struct Catalog {
    /// Display order: main artist, year, album, track.
    songs: Vec<Song>,
    by_id: HashMap<i64, usize>,
    albums: Vec<Album>,
    /// Lowercase artist name -> the songs they are credited on.
    by_artist: HashMap<String, Vec<usize>>,
    artists: Vec<ArtistInfo>,
    categories: Vec<CategoryInfo>,
    /// Our album id -> its Deezer id, and a lowercase artist name -> theirs: what the picture files are named after.
    album_deezer: HashMap<i64, i64>,
    artist_deezer: HashMap<String, i64>,
}

/// The lowercase form names are compared in.
fn fold(s: &str) -> String {
    s.trim().to_lowercase()
}

/// The form a search compares in: [`fold`] without accents (`ą` is `a`).
fn plain(s: &str) -> String {
    songbook_core::plain(s)
}

impl Catalog {
    /// Reads the index and the audio folder.
    pub async fn load(index: &Index, data: &Path) -> Result<Self> {
        let files = file_index(data);
        let db = &mut index.read().await?;
        {
            let mut rows = Vec::new();
            for r in db
                .query(
                    "SELECT id, title, filename, duration, rank, yt_views FROM songs",
                    &[],
                )
                .await?
                .rows
            {
                let stem = str_at(&r, 2);
                if let Some(file) = pick(files.get(&stem)) {
                    rows.push(SongRow {
                        id: int_at(&r, 0),
                        title: str_at(&r, 1),
                        file,
                        duration: int_at(&r, 3),
                        rating: popularity(opt_int_at(&r, 4), opt_int_at(&r, 5)),
                    });
                }
            }
            // How many fans Deezer gives the artists that are followed: the only measure of fame for songs that have no
            // rating of their own.
            let mut fans: HashMap<String, u64> = HashMap::new();
            for r in db.query("SELECT name, fans FROM follows", &[]).await?.rows {
                if let Ok(n) = str_at(&r, 1).trim().parse::<u64>() {
                    fans.insert(fold(&str_at(&r, 0)), n);
                }
            }
            let mut who: HashMap<i64, Vec<String>> = HashMap::new();
            for r in db
                .query(
                    "SELECT sa.song_id, a.name FROM song_artist sa JOIN artists a ON a.id = sa.artist_id ORDER BY sa.song_id, sa.position",
                    &[],
                ).await?
                .rows
            {
                who.entry(int_at(&r, 0)).or_default().push(str_at(&r, 1));
            }
            let mut cats: HashMap<i64, Vec<String>> = HashMap::new();
            for r in db
                .query(
                    "SELECT st.song_id, t.name FROM song_tag st JOIN tags t ON t.id = st.tag_id WHERE t.kind = 'category' ORDER BY t.name COLLATE \"C\"",
                    &[],
                ).await?
                .rows
            {
                cats.entry(int_at(&r, 0)).or_default().push(str_at(&r, 1));
            }
            // Every album a song is on: the first of them (a real album before a single, then the earliest) is where
            // the song is said to be.
            let links = db
                .query(
                    "SELECT sl.song_id, al.id, al.title, al.year, al.type, COALESCE(o.listed_as, o.name, ''), sl.track_no \
                     FROM song_album sl JOIN albums al ON al.id = sl.album_id LEFT JOIN artists o ON o.id = al.owner_id \
                     ORDER BY sl.song_id, CASE al.type WHEN 'album' THEN 0 WHEN 'ep' THEN 1 ELSE 2 END, al.year, al.id",
                    &[],
                ).await?
                .rows;
            let mut album_deezer = HashMap::new();
            for r in db
                .query(
                    "SELECT id, deezer_id FROM albums WHERE deezer_id IS NOT NULL",
                    &[],
                )
                .await?
                .rows
            {
                album_deezer.insert(int_at(&r, 0), int_at(&r, 1));
            }
            let mut artist_deezer = HashMap::new();
            for r in db
                .query(
                    "SELECT name, deezer_id FROM artists WHERE deezer_id IS NOT NULL",
                    &[],
                )
                .await?
                .rows
            {
                artist_deezer.insert(fold(&str_at(&r, 0)), int_at(&r, 1));
            }
            let mut catalog = build(rows, &who, &cats, &links, &fans);
            catalog.album_deezer = album_deezer;
            catalog.artist_deezer = artist_deezer;
            Ok(catalog)
        }
    }

    #[must_use]
    pub fn overview(&self) -> Overview {
        Overview {
            songs: u32::try_from(self.songs.len()).unwrap_or(u32::MAX),
            albums: u32::try_from(self.albums.len()).unwrap_or(u32::MAX),
            artists: self.artists.clone(),
            categories: self.categories.clone(),
            admin: false,
        }
    }

    /// The indexes of the songs a query picks, in play order (`sort=popular`: the most popular first).
    fn pick(&self, q: &SongQuery) -> Vec<usize> {
        let mut picked = self.pick_unsorted(q);
        if q.sort == "popular" {
            picked.sort_by_key(|i| std::cmp::Reverse(self.songs[*i].score));
        }
        picked
    }

    fn pick_unsorted(&self, q: &SongQuery) -> Vec<usize> {
        if !q.ids.trim().is_empty() {
            return q
                .ids
                .split(',')
                .filter_map(|id| id.trim().parse::<i64>().ok())
                .filter_map(|id| self.by_id.get(&id).copied())
                .collect();
        }
        if q.album != 0 {
            return self
                .albums
                .iter()
                .find(|a| a.info.id == q.album)
                .map(|a| a.songs.clone())
                .unwrap_or_default();
        }
        if !q.artist.trim().is_empty() {
            let mut songs = self
                .by_artist
                .get(&fold(&q.artist))
                .cloned()
                .unwrap_or_default();
            // An artist's songs by year, then album, then track: their discography, not the order of the main artists.
            songs.sort_by(|a, b| {
                let (a, b) = (&self.songs[*a], &self.songs[*b]);
                (&a.info.year, &a.info.album, a.track).cmp(&(&b.info.year, &b.info.album, b.track))
            });
            return songs;
        }
        if !q.category.trim().is_empty() {
            let wanted = fold(&q.category);
            return (0..self.songs.len())
                .filter(|i| self.songs[*i].categories.iter().any(|c| fold(c) == wanted))
                .collect();
        }
        if !q.q.trim().is_empty() {
            let words: Vec<String> = q.q.split_whitespace().map(plain).collect();
            return (0..self.songs.len())
                .filter(|i| words.iter().all(|w| self.songs[*i].haystack.contains(w)))
                .collect();
        }
        (0..self.songs.len()).collect()
    }

    #[must_use]
    pub fn songs(&self, q: &SongQuery) -> SongList {
        let picked = self.pick(q);
        let limit = if q.limit == 0 {
            SONGS
        } else {
            q.limit as usize
        };
        SongList {
            total: u32::try_from(picked.len()).unwrap_or(u32::MAX),
            songs: picked
                .iter()
                .take(limit)
                .map(|i| self.songs[*i].info.clone())
                .collect(),
        }
    }

    #[must_use]
    pub fn ids(&self, q: &SongQuery) -> SongIds {
        let picked = self.pick(q);
        let limit = if q.limit == 0 {
            IDS
        } else {
            (q.limit as usize).min(IDS)
        };
        SongIds {
            total: u32::try_from(picked.len()).unwrap_or(u32::MAX),
            ids: picked
                .iter()
                .take(limit)
                .map(|i| self.songs[*i].info.id)
                .collect(),
        }
    }

    #[must_use]
    pub fn albums(&self, q: &AlbumQuery) -> Vec<AlbumInfo> {
        let (artist, category) = (fold(&q.artist), fold(&q.category));
        let words: Vec<String> = q.q.split_whitespace().map(plain).collect();
        let limit = if q.limit == 0 {
            ALBUMS
        } else {
            q.limit as usize
        };
        self.albums
            .iter()
            .filter(|a| artist.is_empty() || fold(&a.info.artist) == artist)
            .filter(|a| category.is_empty() || a.categories.iter().any(|c| fold(c) == category))
            .filter(|a| {
                let hay = plain(&format!("{} {}", a.info.title, a.info.artist));
                words.iter().all(|w| hay.contains(w))
            })
            .take(limit)
            .map(|a| a.info.clone())
            .collect()
    }

    /// What the search box finds: a few of each kind.
    #[must_use]
    pub fn search(&self, text: &str) -> Search {
        let words: Vec<String> = text.split_whitespace().map(plain).collect();
        if words.is_empty() {
            return Search::default();
        }
        let songs = self.songs(&SongQuery {
            q: text.to_string(),
            limit: 25,
            ..SongQuery::default()
        });
        let albums = self.albums(&AlbumQuery {
            q: text.to_string(),
            limit: 12,
            ..AlbumQuery::default()
        });
        let artists = self
            .artists
            .iter()
            .filter(|a| {
                let name = plain(&a.name);
                words.iter().all(|w| name.contains(w))
            })
            .take(12)
            .cloned()
            .collect();
        Search {
            songs: songs.songs,
            albums,
            artists,
        }
    }

    /// What to listen to: popular songs, mixes, artists to try, songs to hear again and the newest albums. `seed` (the day)
    /// changes the picks that are not the plain best.
    #[must_use]
    pub fn home(&self, seed: u64) -> Home {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let mut by_score: Vec<usize> = (0..self.songs.len()).collect();
        by_score.sort_by_key(|i| std::cmp::Reverse(self.songs[*i].score));
        let main = |i: usize| {
            self.songs[i]
                .info
                .artists
                .first()
                .map(|n| fold(n))
                .unwrap_or_default()
        };
        let info = |i: usize| self.songs[i].info.clone();

        // The best, a few of one artist at most.
        let mut taken: HashMap<String, u32> = HashMap::new();
        let popular: Vec<SongInfo> = by_score
            .iter()
            .copied()
            .filter(|i| {
                let n = taken.entry(main(*i)).or_default();
                *n += 1;
                *n <= 2
            })
            .take(12)
            .map(info)
            .collect();

        // Artists: the well known ones (by their best three songs), a different dozen each day.
        let mut fame: Vec<(u32, &String)> = self
            .by_artist
            .iter()
            .map(|(key, list)| {
                let mut scores: Vec<u32> = list.iter().map(|i| self.songs[*i].score).collect();
                scores.sort_unstable_by(|a, b| b.cmp(a));
                (scores.iter().take(3).sum::<u32>(), key)
            })
            .collect();
        fame.sort_by(|a, b| b.cmp(a));
        let mut pool: Vec<&String> = fame.into_iter().take(60).map(|(_, key)| key).collect();
        rng.shuffle(&mut pool);
        let artists = pool
            .into_iter()
            .take(12)
            .filter_map(|key| self.artists.iter().find(|a| fold(&a.name) == **key))
            .cloned()
            .collect();

        // Songs to hear again: well known (the upper part of the library), but not the very top, one of an artist.
        let from = by_score.len() / 12;
        let to = (by_score.len() / 2).max(from + 1).min(by_score.len());
        let mut pool: Vec<usize> = by_score[from.min(to)..to].to_vec();
        rng.shuffle(&mut pool);
        let mut seen: HashSet<String> = HashSet::new();
        let discover = pool
            .into_iter()
            .filter(|i| seen.insert(main(*i)))
            .take(12)
            .map(info)
            .collect();

        // Mixes: the whole library, then each category that is big enough to make one.
        let mut mixes = Vec::new();
        let count = |songs: usize, most: usize| u32::try_from(songs.min(most)).unwrap_or(u32::MAX);
        if self.songs.len() >= 20 {
            mixes.push(Mix {
                name: "Daily Mix".into(),
                kind: "mix".into(),
                category: String::new(),
                songs: count(self.songs.len(), 150),
            });
        }
        for c in &self.categories {
            let size = usize::try_from(c.songs).unwrap_or(0);
            if size < 10 {
                continue;
            }
            mixes.push(Mix {
                name: format!("{} Mix", c.name),
                kind: "mix".into(),
                category: c.name.clone(),
                songs: count(size, 100),
            });
            mixes.push(Mix {
                name: format!("Best of {}", c.name),
                kind: "best".into(),
                category: c.name.clone(),
                songs: count(size, 50),
            });
        }

        // The newest albums that are more than a single.
        let mut fresh: Vec<&Album> = self.albums.iter().filter(|a| a.info.songs >= 3).collect();
        fresh.sort_by(|a, b| {
            (&b.info.year, fold(&a.info.artist), fold(&a.info.title)).cmp(&(
                &a.info.year,
                fold(&b.info.artist),
                fold(&b.info.title),
            ))
        });
        let albums = fresh.into_iter().take(12).map(|a| a.info.clone()).collect();

        Home {
            popular,
            mixes,
            artists,
            discover,
            albums,
        }
    }

    /// The picture files to try, best first, for something: an `album` (by our id), an `artist` (by name) or a `song` (by
    /// id: its album's cover, which for a single is the single's own, then its main artist's picture). They are names in
    /// [`crate::images::dir`].
    #[must_use]
    pub fn art(&self, kind: &str, key: &str) -> Vec<String> {
        let album = |id: i64| {
            self.album_deezer
                .get(&id)
                .map(|d| crate::images::album_name(*d))
        };
        let artist = |name: &str| {
            self.artist_deezer
                .get(&fold(name))
                .map(|d| crate::images::artist_name(*d))
        };
        match kind {
            "album" => key.parse().ok().and_then(album).into_iter().collect(),
            "artist" => artist(key).into_iter().collect(),
            "song" => {
                let Some(song) = key.parse().ok().and_then(|id| self.by_id.get(&id)) else {
                    return Vec::new();
                };
                let info = &self.songs[*song].info;
                album(info.album_id)
                    .into_iter()
                    .chain(info.artists.first().and_then(|a| artist(a)))
                    .collect()
            }
            _ => Vec::new(),
        }
    }

    /// The audio file of a song, with its extension; `None` for a song with no file (or not in the library).
    #[must_use]
    pub fn file(&self, id: i64) -> Option<&str> {
        self.by_id.get(&id).map(|i| self.songs[*i].file.as_str())
    }
}

/// A small, repeatable random sequence: the day's picks are the same for everyone the whole day.
struct Rng(u64);

impl Rng {
    const fn next(&mut self) -> u64 {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn shuffle<T>(&mut self, list: &mut [T]) {
        for i in (1..list.len()).rev() {
            let j = (self.next() % (i as u64 + 1)) as usize;
            list.swap(i, j);
        }
    }
}

/// The file to play among those that share a name: an `mp3` first (what convert makes), else the first by name.
fn pick(names: Option<&Vec<String>>) -> Option<String> {
    let names = names?;
    names
        .iter()
        .find(|n| n.to_lowercase().ends_with(".mp3"))
        .or_else(|| names.iter().min())
        .cloned()
}

/// A song of the index that has a file.
struct SongRow {
    id: i64,
    title: String,
    file: String,
    duration: i64,
    /// Its popularity as the index has it, if it does.
    rating: Option<u32>,
}

/// Skits, intros and the like are not what anyone means by a popular song, and a remix or a live take is not the song
/// people know: both rank lower than the song's own score says (a fifth, and three quarters of it).
#[must_use]
pub fn adjusted(title: &str, score: u32) -> u32 {
    let t = title.to_lowercase();
    let words: Vec<&str> = t
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    let has = |w: &str| words.contains(&w);
    let filler = [
        "skit",
        "intro",
        "outro",
        "interlude",
        "instrumental",
        "acapella",
        "reprise",
        "freestyle",
    ]
    .iter()
    .any(|w| has(w))
        || t.contains("a cappella");
    let version = [
        "remix", "live", "version", "edit", "mix", "demo", "radio", "clean", "dirty",
    ]
    .iter()
    .any(|w| has(w));
    if filler {
        score / 5
    } else if version {
        score * 3 / 4
    } else {
        score
    }
}

/// How popular a song with no rating of its own is: its best-known artist (log scale of Deezer's fans, up to 70) and the
/// albums it is on beyond the first (up to 30).
fn guessed(fame: f64, albums: usize) -> u32 {
    let extra = albums.saturating_sub(1).min(3) as f64 / 3.0;
    (fame.mul_add(70.0, extra * 30.0)).round() as u32
}

fn build(
    rows: Vec<SongRow>,
    who: &HashMap<i64, Vec<String>>,
    cats: &HashMap<i64, Vec<String>>,
    links: &[Vec<crate::index::Value>],
    fans: &HashMap<String, u64>,
) -> Catalog {
    let top_fans = fans.values().copied().max().unwrap_or(0);
    let fame = |artists: &[String]| -> f64 {
        if top_fans == 0 {
            return 0.0;
        }
        artists
            .iter()
            .filter_map(|a| fans.get(&fold(a)))
            .map(|n| (*n as f64).ln_1p() / (top_fans as f64).ln_1p())
            .fold(0.0, f64::max)
    };
    let mut on_albums: HashMap<i64, usize> = HashMap::new();
    for link in links {
        *on_albums.entry(int_at(link, 0)).or_default() += 1;
    }
    // The album of each song (its first link) and, per album, its songs with their track numbers.
    let mut first: HashMap<i64, usize> = HashMap::new();
    for (at, link) in links.iter().enumerate() {
        first.entry(int_at(link, 0)).or_insert(at);
    }
    let mut songs: Vec<Song> = rows
        .into_iter()
        .map(|row| {
            let SongRow {
                id,
                title,
                file,
                duration,
                rating,
            } = row;
            let artists = who.get(&id).cloned().unwrap_or_default();
            let score = adjusted(
                &title,
                rating.unwrap_or_else(|| {
                    guessed(fame(&artists), on_albums.get(&id).copied().unwrap_or(0))
                }),
            );
            let link = first.get(&id).map(|at| &links[*at]);
            let (album_id, album, year) = link.map_or((0, String::new(), String::new()), |l| {
                (int_at(l, 1), str_at(l, 2), str_at(l, 3))
            });
            let track = link.and_then(|l| opt_int_at(l, 6)).unwrap_or(0);
            let categories = cats.get(&id).cloned().unwrap_or_default();
            let haystack = plain(&format!("{title} {album} {}", artists.join(" ")));
            Song {
                info: SongInfo {
                    id,
                    title,
                    artists,
                    album,
                    album_id,
                    year,
                    duration: u32::try_from(duration.max(0)).unwrap_or(0),
                },
                file,
                track,
                score,
                categories,
                haystack,
            }
        })
        .collect();
    songs.sort_by(|a, b| {
        let key = |s: &Song| {
            (
                s.info.artists.first().map(|n| fold(n)).unwrap_or_default(),
                s.info.year.clone(),
                s.info.album.clone(),
                s.track,
                fold(&s.info.title),
            )
        };
        key(a).cmp(&key(b))
    });
    let by_id: HashMap<i64, usize> = songs
        .iter()
        .enumerate()
        .map(|(i, s)| (s.info.id, i))
        .collect();

    // Albums: every link of a playable song.
    let mut albums: Vec<Album> = Vec::new();
    let mut at_album: HashMap<i64, usize> = HashMap::new();
    let mut tracked: Vec<(usize, i64, usize)> = Vec::new();
    for link in links {
        let Some(song) = by_id.get(&int_at(link, 0)).copied() else {
            continue;
        };
        let id = int_at(link, 1);
        let slot = *at_album.entry(id).or_insert_with(|| {
            albums.push(Album {
                info: AlbumInfo {
                    id,
                    title: str_at(link, 2),
                    artist: str_at(link, 5),
                    year: str_at(link, 3),
                    kind: str_at(link, 4),
                    songs: 0,
                },
                songs: Vec::new(),
                categories: HashSet::new(),
            });
            albums.len() - 1
        });
        tracked.push((slot, opt_int_at(link, 6).unwrap_or(0), song));
    }
    tracked.sort_unstable();
    for (slot, _, song) in tracked {
        let album = &mut albums[slot];
        if !album.songs.contains(&song) {
            album.songs.push(song);
            album
                .categories
                .extend(songs[song].categories.iter().cloned());
        }
    }
    for album in &mut albums {
        album.info.songs = u32::try_from(album.songs.len()).unwrap_or(u32::MAX);
    }
    albums.sort_by(|a, b| {
        (fold(&a.info.artist), &a.info.year, fold(&a.info.title)).cmp(&(
            fold(&b.info.artist),
            &b.info.year,
            fold(&b.info.title),
        ))
    });

    // Artists: whoever is credited, counted once per song.
    let mut by_artist: HashMap<String, Vec<usize>> = HashMap::new();
    let mut names: HashMap<String, String> = HashMap::new();
    for (i, s) in songs.iter().enumerate() {
        for name in &s.info.artists {
            let key = fold(name);
            let list = by_artist.entry(key.clone()).or_default();
            if list.last() != Some(&i) {
                list.push(i);
            }
            names.entry(key).or_insert_with(|| name.clone());
        }
    }
    let mut artist_albums: HashMap<String, u32> = HashMap::new();
    for a in &albums {
        *artist_albums.entry(fold(&a.info.artist)).or_default() += 1;
    }
    let mut artists: Vec<ArtistInfo> = by_artist
        .iter()
        .map(|(key, list)| ArtistInfo {
            name: names[key].clone(),
            songs: u32::try_from(list.len()).unwrap_or(u32::MAX),
            albums: artist_albums.get(key).copied().unwrap_or(0),
        })
        .collect();
    artists.sort_by_key(|a| fold(&a.name));

    let mut counts: HashMap<&str, u32> = HashMap::new();
    for s in &songs {
        for c in &s.categories {
            *counts.entry(c.as_str()).or_default() += 1;
        }
    }
    let mut categories: Vec<CategoryInfo> = counts
        .into_iter()
        .map(|(name, songs)| CategoryInfo {
            name: name.to_string(),
            songs,
        })
        .collect();
    categories.sort_by_key(|c| fold(&c.name));

    Catalog {
        songs,
        by_id,
        albums,
        by_artist,
        artists,
        categories,
        album_deezer: HashMap::new(),
        artist_deezer: HashMap::new(),
    }
}

/// When it was built, the index generation it was built for, and the catalog.
type Built = (Instant, u64, Arc<Catalog>);

/// The catalog, rebuilt when the index changed or the last look at the files is old.
#[derive(Debug, Clone, Default)]
pub struct Library {
    built: Arc<Mutex<Option<Built>>>,
}

impl Library {
    pub async fn catalog(&self, index: &Index, data: PathBuf) -> Result<Arc<Catalog>> {
        let generation = index.generation();
        if let Some((at, built_for, catalog)) = self
            .built
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            && built_for == generation
            && at.elapsed() < FRESH
        {
            return Ok(catalog);
        }
        let catalog = Arc::new(Catalog::load(index, &data).await?);
        *self.built.lock().unwrap_or_else(PoisonError::into_inner) =
            Some((Instant::now(), generation, Arc::clone(&catalog)));
        Ok(catalog)
    }
}
