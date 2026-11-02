use crate::support::*;
use songbook_core::{AlbumQuery, SongQuery};
use songbook_service::index::songs::{
    AlbumInfo, link_album, upsert_album, upsert_artist, upsert_song,
};
use songbook_service::index::{Index, int, str_at, text};
use songbook_service::library::Catalog;
use songbook_service::playlists::{Playlists, Refused};
use songbook_service::routers::api::listen::{content_type, parse_range};

fn album(id: i64, title: &str, year: &str) -> AlbumInfo {
    AlbumInfo {
        deezer_id: id,
        title: title.into(),
        release_date: format!("{year}-05-01"),
        record_type: "album".into(),
        genres: vec![],
    }
}

fn names(list: &[(i64, &str)]) -> Vec<(i64, String)> {
    list.iter().map(|(i, n)| (*i, (*n).to_string())).collect()
}

/// Nate Dogg with two albums and three songs (one with Snoop Dogg); only the first two have a file.
fn library(dir: &Scratch) -> (Index, Vec<i64>) {
    let index = index_in(dir);
    let ids = index
        .with(|db| {
            let nate = upsert_artist(db, 359, "Nate Dogg", Some("Nate Dogg"))?;
            let a1 = upsert_album(db, &album(1, "Ghetto Preacher", "2001"), nate)?;
            let a2 = upsert_album(db, &album(2, "Elements of Rap", "2005"), nate)?;
            let one = upsert_song(
                db,
                &names(&[(359, "Nate Dogg")]),
                "Nobody Does It Better",
                Some(269),
                1,
            )?;
            link_album(db, one, a1, 1)?;
            let two = upsert_song(
                db,
                &names(&[(359, "Nate Dogg"), (1, "Snoop Dogg")]),
                "Never Leave Me Alone",
                Some(250),
                2,
            )?;
            link_album(db, two, a2, 1)?;
            let three = upsert_song(
                db,
                &names(&[(359, "Nate Dogg")]),
                "No File Yet",
                Some(200),
                3,
            )?;
            link_album(db, three, a2, 2)?;
            db.execute(
                "INSERT INTO tags(kind, name) VALUES ('category', 'Rap')",
                &[],
            )?;
            let tag = db.last_insert_rowid();
            db.execute(
                "INSERT INTO song_tag(song_id, tag_id) VALUES (?, ?)",
                &[int(one), int(tag)],
            )?;
            Ok(vec![one, two, three])
        })
        .unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    for id in &ids[..2] {
        let stem = index
            .with(|db| {
                Ok(str_at(
                    &db.query("SELECT filename FROM songs WHERE id = ?", &[int(*id)])?
                        .rows[0],
                    0,
                ))
            })
            .unwrap();
        std::fs::write(data.join(format!("{stem}.mp3")), b"audio").unwrap();
    }
    (index, ids)
}

#[quench_test::test]
fn only_songs_with_a_file_are_in_the_catalog_and_they_are_found_by_album_artist_category_and_words()
{
    let dir = Scratch::new();
    let (index, ids) = library(&dir);
    let catalog = Catalog::load(&index, &dir.path().join("data")).unwrap();

    let all = catalog.songs(&SongQuery::default());
    assert_eq!(all.total, 2, "the song with no file is not offered");
    assert_eq!(catalog.overview().songs, 2);

    let by_album = catalog.songs(&SongQuery {
        album: catalog.albums(&AlbumQuery::default())[1].id,
        ..SongQuery::default()
    });
    assert_eq!(by_album.songs.len(), 1);
    assert_eq!(by_album.songs[0].title, "Never Leave Me Alone");

    // Snoop Dogg is credited on one song, Nate Dogg on both, oldest album first.
    let snoop = catalog.songs(&SongQuery {
        artist: "snoop dogg".into(),
        ..SongQuery::default()
    });
    assert_eq!(snoop.total, 1);
    let nate = catalog.songs(&SongQuery {
        artist: "Nate Dogg".into(),
        ..SongQuery::default()
    });
    assert_eq!(
        nate.songs
            .iter()
            .map(|s| s.title.as_str())
            .collect::<Vec<_>>(),
        ["Nobody Does It Better", "Never Leave Me Alone"]
    );

    let rap = catalog.ids(&SongQuery {
        category: "rap".into(),
        ..SongQuery::default()
    });
    assert_eq!(rap.ids, vec![ids[0]]);
    let words = catalog.songs(&SongQuery {
        q: "never alone".into(),
        ..SongQuery::default()
    });
    assert_eq!(words.total, 1);

    // Ids come back in the order asked, and an unknown or file-less one is left out.
    let asked = format!("{},9999,{}", ids[1], ids[0]);
    let by_ids = catalog.ids(&SongQuery {
        ids: asked,
        ..SongQuery::default()
    });
    assert_eq!(by_ids.ids, vec![ids[1], ids[0]]);
    let third = catalog.ids(&SongQuery {
        ids: ids[2].to_string(),
        ..SongQuery::default()
    });
    assert!(third.ids.is_empty());

    assert_eq!(
        catalog.file(ids[0]).unwrap().rsplit_once('.').unwrap().1,
        "mp3"
    );
    assert!(catalog.file(ids[2]).is_none());

    let overview = catalog.overview();
    assert_eq!(
        overview
            .artists
            .iter()
            .map(|a| a.name.as_str())
            .collect::<Vec<_>>(),
        ["Nate Dogg", "Snoop Dogg"]
    );
    assert_eq!(overview.categories.len(), 1);
    let found = catalog.search("nobody");
    assert_eq!(
        (found.songs.len(), found.albums.len(), found.artists.len()),
        (1, 0, 0)
    );
}

#[quench_test::test]
fn the_catalog_notices_a_new_file_when_it_is_built_again() {
    let dir = Scratch::new();
    let (index, ids) = library(&dir);
    let stem = index
        .with(|db| {
            Ok(str_at(
                &db.query("SELECT filename FROM songs WHERE id = ?", &[int(ids[2])])?
                    .rows[0],
                0,
            ))
        })
        .unwrap();
    std::fs::write(
        dir.path().join("data").join(format!("{stem}.opus")),
        b"audio",
    )
    .unwrap();
    let catalog = Catalog::load(&index, &dir.path().join("data")).unwrap();
    assert_eq!(catalog.overview().songs, 3);
    assert_eq!(
        catalog.file(ids[2]).unwrap().rsplit_once('.').unwrap().1,
        "opus"
    );
    let _ = text("");
}

#[quench_test::test]
fn playlists_belong_to_their_owner_and_keep_their_order() {
    let dir = Scratch::new();
    let lists = Playlists::open(&dir.path().join("user.db")).unwrap();
    let mine = lists.create("ann", "  Road   trip ", 1).unwrap().unwrap();
    assert_eq!(mine.name, "Road trip");
    assert_eq!(
        lists.create("ann", "  ", 1).unwrap().unwrap_err(),
        Refused::Limit("A playlist needs a name.")
    );
    let theirs = lists.create("bob", "Bob's", 1).unwrap().unwrap();

    assert_eq!(
        lists
            .add("ann", mine.id, &[3, 1, 2])
            .unwrap()
            .unwrap()
            .songs,
        3
    );
    assert_eq!(
        lists.add("ann", mine.id, &[1]).unwrap().unwrap().songs,
        4,
        "a song may be on twice"
    );
    assert_eq!(
        lists.songs("ann", mine.id).unwrap().unwrap(),
        vec![3, 1, 2, 1]
    );

    // Not Bob's to read, change or delete.
    assert!(lists.songs("bob", mine.id).unwrap().is_none());
    assert_eq!(
        lists.add("bob", mine.id, &[9]).unwrap().unwrap_err(),
        Refused::Unknown
    );
    assert!(!lists.delete("bob", mine.id).unwrap());
    assert_eq!(lists.list("ann").unwrap().len(), 1);
    assert_eq!(lists.list("bob").unwrap()[0].id, theirs.id);

    assert_eq!(
        lists.set("ann", mine.id, &[2, 3]).unwrap().unwrap().songs,
        2
    );
    assert_eq!(lists.songs("ann", mine.id).unwrap().unwrap(), vec![2, 3]);
    assert_eq!(
        lists.rename("ann", mine.id, "Trip").unwrap().unwrap().name,
        "Trip"
    );
    assert!(lists.delete("ann", mine.id).unwrap());
    assert!(lists.list("ann").unwrap().is_empty());
}

#[quench_test::test]
fn a_range_is_read_the_way_a_player_asks_for_it() {
    assert_eq!(parse_range("bytes=0-99", 1000), Some((0, 99)));
    assert_eq!(parse_range("bytes=900-", 1000), Some((900, 999)));
    assert_eq!(
        parse_range("bytes=0-", 10_000_000),
        Some((0, 4 * 1024 * 1024 - 1)),
        "the rest comes in chunks"
    );
    assert_eq!(parse_range("bytes=-100", 1000), Some((900, 999)));
    assert_eq!(
        parse_range("bytes=500-5000", 1000),
        Some((500, 999)),
        "an end past the file is cut"
    );
    assert_eq!(parse_range("bytes=1000-", 1000), None);
    assert_eq!(parse_range("bytes=9-3", 1000), None);
    assert_eq!(parse_range("items=0-1", 1000), None);
    assert_eq!(parse_range("bytes=0-1", 0), None);
    assert_eq!(content_type("A - B.mp3"), "audio/mpeg");
    assert_eq!(content_type("A - B.OPUS"), "audio/ogg");
    assert_eq!(content_type("noext"), "application/octet-stream");
}

/// Thirty songs with a file: ten artists with three songs each (artist `n` has `n * 1000` fans), two categories, one album per
/// artist whose year grows with `n`.
fn big_library(dir: &Scratch) -> Index {
    let index = index_in(dir);
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    index
        .with(|db| {
            db.execute(
                "INSERT INTO tags(kind, name) VALUES ('category', 'Rap'), ('category', 'Rock')",
                &[],
            )?;
            for n in 1..=10_i64 {
                let name = format!("Artist {n:02}");
                let artist = upsert_artist(db, n, &name, Some(&name))?;
                db.execute(
                    "INSERT INTO follows(name, fans) VALUES (?, ?)",
                    &[text(&name), text(&(n * 1000).to_string())],
                )?;
                let album = upsert_album(
                    db,
                    &album(n, &format!("Album {n:02}"), &format!("{}", 2000 + n)),
                    artist,
                )?;
                for t in 1..=3_i64 {
                    let song = upsert_song(
                        db,
                        &names(&[(n, &name)]),
                        &format!("Song {n:02}-{t}"),
                        Some(200),
                        n * 10 + t,
                    )?;
                    link_album(db, song, album, t)?;
                    let tag = if n % 2 == 0 { 1 } else { 2 };
                    db.execute(
                        "INSERT INTO song_tag(song_id, tag_id) VALUES (?, ?)",
                        &[int(song), int(tag)],
                    )?;
                    let stem = str_at(
                        &db.query("SELECT filename FROM songs WHERE id = ?", &[int(song)])?
                            .rows[0],
                        0,
                    );
                    std::fs::write(data.join(format!("{stem}.mp3")), b"audio").unwrap();
                }
            }
            Ok(())
        })
        .unwrap();
    index
}

#[quench_test::test]
fn the_home_page_offers_the_best_mixes_artists_to_try_and_the_newest_albums() {
    let dir = Scratch::new();
    let index = big_library(&dir);
    let catalog = Catalog::load(&index, &dir.path().join("data")).unwrap();
    let home = catalog.home(1);

    // The best: the best-known artist's songs first, and no artist more than twice.
    assert_eq!(home.popular.len(), 12);
    assert_eq!(home.popular[0].artists[0], "Artist 10");
    let mut per_artist = std::collections::HashMap::<&str, u32>::new();
    for s in &home.popular {
        *per_artist.entry(s.artists[0].as_str()).or_default() += 1;
    }
    assert!(per_artist.values().all(|n| *n <= 2), "{per_artist:?}");

    // Mixes: one for everything, then two for each category that has enough songs.
    let names: Vec<&str> = home.mixes.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "Daily Mix",
            "Rap Mix",
            "Best of Rap",
            "Rock Mix",
            "Best of Rock"
        ]
    );
    let best = &home.mixes[2];
    assert!(!best.shuffled() && home.mixes[1].shuffled());
    let played = catalog.songs(&best.query());
    assert_eq!(played.songs.len(), 15, "a category of fifteen songs");
    assert_eq!(
        played.songs[0].artists[0], "Artist 10",
        "the most popular first"
    );

    // The newest albums first.
    assert_eq!(home.albums[0].title, "Album 10");
    assert!(home.albums.len() <= 12);
    // Songs to hear again are not the very best, and one of an artist.
    assert!(!home.discover.is_empty() && home.discover.len() <= 12);
    assert!(
        home.discover.iter().all(|s| s.id != home.popular[0].id),
        "not the very top song"
    );
    let unique: std::collections::HashSet<_> =
        home.discover.iter().map(|s| &s.artists[0]).collect();
    assert_eq!(unique.len(), home.discover.len());
}

#[quench_test::test]
fn the_days_picks_stay_and_change_with_the_day() {
    let dir = Scratch::new();
    let index = big_library(&dir);
    let catalog = Catalog::load(&index, &dir.path().join("data")).unwrap();
    assert_eq!(catalog.home(5), catalog.home(5));
    let (a, b) = (catalog.home(5), catalog.home(6));
    assert_eq!(a.popular, b.popular, "the plain best does not change");
    assert!(
        a.artists != b.artists || a.discover != b.discover,
        "the picks of another day differ"
    );
}

#[quench_test::test]
fn a_library_with_no_ratings_ranks_songs_by_how_well_known_their_artist_is() {
    let dir = Scratch::new();
    let index = big_library(&dir);
    let catalog = Catalog::load(&index, &dir.path().join("data")).unwrap();
    let top = catalog.songs(&SongQuery {
        sort: "popular".into(),
        limit: 3,
        ..SongQuery::default()
    });
    assert!(top.songs.iter().all(|s| s.artists[0] == "Artist 10"));
    // Without `sort` the library order stays.
    let plain = catalog.songs(&SongQuery {
        limit: 1,
        ..SongQuery::default()
    });
    assert_eq!(plain.songs[0].artists[0], "Artist 01");
}

#[quench_test::test]
fn skits_and_other_versions_rank_below_the_song_itself() {
    use songbook_service::library::adjusted;
    assert_eq!(adjusted("Stan", 80), 80);
    assert_eq!(adjusted("Steve Berman (Skit)", 80), 16);
    assert_eq!(adjusted("Intro", 80), 16);
    assert_eq!(adjusted("Bear With Me (Instrumental)", 80), 16);
    assert_eq!(adjusted("Knuckle up (Matt U Remix)", 80), 60);
    assert_eq!(adjusted("Lose Yourself - Live", 80), 60);
    assert_eq!(
        adjusted("Introduction to Love", 80),
        80,
        "a word, not a part of one"
    );
    assert_eq!(adjusted("Remixed Emotions", 80), 80);
}

// ── Pictures ─────────────────────────────────────────────────────────────────

use songbook_service::images;
use songbook_service::jobs::Jobs;
use songbook_service::routers::api::art::{content_type as image_type, etag, width_for};

/// An app (as the job tests build one) over `dir`, whose index has what `seed` puts in it.
fn app_over(dir: &Scratch, answers: &[(&str, &str)]) -> songbook_service::app::App {
    app_with(dir, answers)
}

fn seed_pictures(app: &songbook_service::app::App) {
    app.index
        .with(|db| {
            // Nate Dogg (Deezer 359) with an album and a single, and Snoop Dogg (Deezer 1) who has no picture.
            let nate = upsert_artist(db, 359, "Nate Dogg", Some("Nate Dogg"))?;
            let snoop = upsert_artist(db, 1, "Snoop Dogg", None)?;
            let album_id = upsert_album(db, &album(11, "Ghetto Preacher", "1999"), nate)?;
            let mut single = album(12, "Lonely Single", "2001");
            single.record_type = "single".into();
            let single_id = upsert_album(db, &single, nate)?;
            let a = upsert_song(
                db,
                &names(&[(359, "Nate Dogg")]),
                "On The Album",
                Some(200),
                1,
            )?;
            link_album(db, a, album_id, 1)?;
            let b = upsert_song(
                db,
                &names(&[(359, "Nate Dogg")]),
                "On The Single",
                Some(200),
                2,
            )?;
            link_album(db, b, single_id, 1)?;
            let c = upsert_song(db, &names(&[(1, "Snoop Dogg")]), "Alone", Some(200), 3)?;
            let _ = (c, snoop);
            Ok(())
        })
        .unwrap();
}

const ALBUM_11: &str = r#"{"id":11,"title":"Ghetto Preacher","cover_big":"https://cdn/album11-500.jpg","cover_xl":"https://cdn/album11-1000.jpg"}"#;
const ALBUM_12: &str =
    r#"{"id":12,"title":"Lonely Single","cover_big":"https://cdn/single12-500.jpg"}"#;
const ARTIST_359: &str =
    r#"{"id":359,"name":"Nate Dogg","picture_big":"https://cdn/nate-500.jpg"}"#;
const ARTIST_1: &str = r#"{"id":1,"name":"Snoop Dogg","picture_big":"https://cdn/images/artist//500x500-000000-80-0-0.jpg"}"#;

#[quench_test::test]
async fn the_images_job_saves_what_deezer_has_and_remembers_what_it_has_not() {
    let dir = Scratch::new();
    let app = app_over(
        &dir,
        &[
            ("album/11", ALBUM_11),
            ("album/12", ALBUM_12),
            ("artist/359", ARTIST_359),
            ("artist/1", ARTIST_1),
        ],
    );
    seed_pictures(&app);
    let jobs = Jobs::default();
    let done = std::sync::Arc::new(std::sync::Mutex::new(None));
    let (a, slot) = (app.clone(), done.clone());
    assert!(jobs.start("images", move |log| async move {
        let r = images::run(&a, false, &log).await;
        *slot.lock().unwrap() = Some(r.clone());
        r
    }));
    while jobs.running() {
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    assert_eq!(done.lock().unwrap().clone(), Some(Ok(())));
    let pictures = images::dir(&app.config);
    for name in [
        images::album_name(11),
        images::album_name(12),
        images::artist_name(359),
    ] {
        let bytes =
            std::fs::read(pictures.join(&name)).unwrap_or_else(|_| panic!("{name} was saved"));
        assert!(images::is_picture(&bytes));
    }
    let single =
        String::from_utf8_lossy(&std::fs::read(pictures.join(images::album_name(12))).unwrap())
            .to_string();
    assert!(
        single.contains("single12-500.jpg"),
        "the single's own cover: {single}"
    );
    // Deezer's stand-in picture is no picture: nothing saved, and it is not asked for again.
    assert!(!pictures.join(images::artist_name(1)).exists());
    assert!(
        pictures
            .join(format!("{}.none", images::artist_name(1)))
            .exists()
    );
    let log = jobs.read(0).log;
    assert!(
        log.contains("2 saved") || log.contains("albums: 2 saved"),
        "{log}"
    );

    // A second run has nothing to fetch.
    let a = app.clone();
    let again = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let sink = again.clone();
    jobs.start("images", move |log| async move {
        let r = images::run(&a, false, &log).await;
        *sink.lock().unwrap() = String::new();
        r
    });
    while jobs.running() {
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    let log = jobs.read(0).log;
    assert!(log.contains("albums: 0 saved, 2 already there"), "{log}");
}

#[quench_test::test]
async fn retrying_asks_again_for_what_deezer_had_none_of() {
    let dir = Scratch::new();
    let app = app_over(
        &dir,
        &[
            ("album/11", ALBUM_11),
            ("artist/359", ARTIST_359),
            ("artist/1", ARTIST_1),
        ],
    );
    seed_pictures(&app);
    let pictures = images::dir(&app.config);
    std::fs::create_dir_all(&pictures).unwrap();
    std::fs::write(
        pictures.join(format!("{}.none", images::album_name(12))),
        b"",
    )
    .unwrap();
    let jobs = Jobs::default();
    let a = app.clone();
    jobs.start("images", move |log| async move {
        images::run(&a, false, &log).await
    });
    while jobs.running() {
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    assert!(
        !pictures.join(images::album_name(12)).exists(),
        "the marker stands"
    );
    let a = app.clone();
    jobs.start("images", move |log| async move {
        images::run(&a, true, &log).await
    });
    while jobs.running() {
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    // With the markers gone the album is asked for again (the canned answers have no `album/12`: still no cover).
    assert!(!pictures.join(images::album_name(12)).exists());
    assert!(jobs.read(0).log.contains("asking again for"));
}

#[quench_test::test]
fn a_song_has_its_albums_cover_a_singles_own_and_else_its_artists_picture() {
    let dir = Scratch::new();
    let app = app_over(&dir, &[]);
    seed_pictures(&app);
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    for stem in [
        "Nate Dogg - On The Album",
        "Nate Dogg - On The Single",
        "Snoop Dogg - Alone",
    ] {
        std::fs::write(data.join(format!("{stem}.mp3")), b"a").unwrap();
    }
    let catalog = Catalog::load(&app.index, &data).unwrap();
    let id = |title: &str| {
        catalog
            .songs(&SongQuery {
                q: title.into(),
                ..SongQuery::default()
            })
            .songs[0]
            .id
    };
    assert_eq!(
        catalog.art("song", &id("On The Album").to_string()),
        ["album-11.jpg", "artist-359.jpg"]
    );
    assert_eq!(
        catalog.art("song", &id("On The Single").to_string()),
        ["album-12.jpg", "artist-359.jpg"]
    );
    assert_eq!(
        catalog.art("song", &id("Alone").to_string()),
        ["artist-1.jpg"],
        "no album: the artist's picture"
    );
    assert_eq!(catalog.art("artist", "nate dogg"), ["artist-359.jpg"]);
    assert!(catalog.art("artist", "nobody").is_empty());
    assert!(catalog.art("song", "999999").is_empty());
    let album_id = catalog.albums(&AlbumQuery::default())[0].id;
    assert_eq!(catalog.art("album", &album_id.to_string()).len(), 1);
}

#[quench_test::test]
fn thumbnails_are_made_in_a_few_widths_and_the_type_comes_from_the_file() {
    assert_eq!(width_for(0), None);
    assert_eq!(
        (
            width_for(40),
            width_for(64),
            width_for(100),
            width_for(300),
            width_for(500)
        ),
        (Some(64), Some(64), Some(160), Some(320), None)
    );
    assert_eq!(image_type(&[0x89, b'P', b'N', b'G', 1]), "image/png");
    assert_eq!(image_type(&[0xFF, 0xD8, 0xFF]), "image/jpeg");
}

#[quench_test::test]
fn a_picture_is_named_by_its_file_and_width_so_a_device_can_ask_whether_it_changed() {
    assert_eq!(
        etag(41_000, 1_700_000_000, Some(160)),
        etag(41_000, 1_700_000_000, Some(160))
    );
    assert_ne!(
        etag(41_000, 1_700_000_000, Some(160)),
        etag(41_000, 1_700_000_000, Some(320))
    );
    assert_ne!(
        etag(41_000, 1_700_000_000, None),
        etag(41_001, 1_700_000_000, None),
        "a new file"
    );
    assert_ne!(
        etag(41_000, 1_700_000_000, None),
        etag(41_000, 1_700_000_001, None),
        "saved again"
    );
    assert!(etag(1, 2, None).starts_with('"'), "an ETag is quoted");
}
