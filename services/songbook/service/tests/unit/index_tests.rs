use crate::support::*;
use songbook_core::Filter;
use songbook_service::index::songs::{
    AlbumInfo, link_album, prune, retag, upsert_album, upsert_artist, upsert_song,
};
use songbook_service::index::{Index, browse, follow, int, text};
use std::collections::HashMap;

fn album(id: i64, title: &str, genres: &[&str]) -> AlbumInfo {
    AlbumInfo {
        deezer_id: id,
        title: title.into(),
        release_date: "2001-05-01".into(),
        record_type: "album".into(),
        genres: genres.iter().map(|g| (*g).to_string()).collect(),
    }
}

fn names(list: &[(i64, &str)]) -> Vec<(i64, String)> {
    list.iter().map(|(i, n)| (*i, (*n).to_string())).collect()
}

/// Nate Dogg (followed) with two albums sharing a song, and one song with Snoop Dogg.
fn catalogue(index: &Index) -> (i64, i64) {
    index
        .with(|db| {
            let nate = upsert_artist(db, 359, "Nate Dogg", Some("Nate Dogg"))?;
            let a1 = upsert_album(db, &album(1, "Ghetto Preacher", &["Rap/Hip Hop"]), nate)?;
            let a2 = upsert_album(
                db,
                &album(2, "Greatest Hits (Deluxe Edition)", &["Rap/Hip Hop", "R&B"]),
                nate,
            )?;
            let s1 = upsert_song(
                db,
                &names(&[(359, "Nate Dogg")]),
                "Nobody Does It Better",
                Some(269),
                100,
            )?;
            link_album(db, s1, a1, 1)?;
            let again = upsert_song(
                db,
                &names(&[(359, "Nate Dogg")]),
                "Nobody Does It Better - 2004 Remaster",
                Some(270),
                200,
            )?;
            assert_eq!(again, s1, "the remaster is the same song");
            link_album(db, again, a2, 4)?;
            let s2 = upsert_song(
                db,
                &names(&[(359, "Nate Dogg"), (1, "Snoop Dogg")]),
                "Never Leave Me Alone (feat. Snoop Dogg)",
                Some(250),
                300,
            )?;
            link_album(db, s2, a2, 5)?;
            Ok((s1, s2))
        })
        .unwrap()
}

#[quench_test::test]
fn a_song_on_two_albums_is_one_row_with_an_alphabetical_file_name() {
    let dir = Scratch::new();
    let index = index_in(&dir);
    let (s1, s2) = catalogue(&index);
    assert_ne!(s1, s2);
    index
        .with(|db| {
            let files = db.query("SELECT filename FROM songs ORDER BY id", &[])?;
            let files: Vec<String> = files
                .rows
                .iter()
                .map(|r| songbook_service::index::str_at(r, 0))
                .collect();
            assert_eq!(
                files,
                vec![
                    "Nate Dogg - Nobody Does It Better",
                    "Nate Dogg, Snoop Dogg - Never Leave Me Alone"
                ]
            );
            assert_eq!(
                db.query("SELECT COUNT(*) FROM song_album", &[])?.rows[0][0],
                quench_sqlite::Value::Integer(3)
            );
            assert_eq!(
                db.query("SELECT COUNT(*) FROM song_track", &[])?.rows[0][0],
                quench_sqlite::Value::Integer(3)
            );
            assert_eq!(
                songbook_service::index::songs::artists_of(db, s2)?,
                vec!["Nate Dogg", "Snoop Dogg"]
            );
            Ok(())
        })
        .unwrap();
}

#[quench_test::test]
fn two_different_songs_with_one_file_name_are_told_apart() {
    let dir = Scratch::new();
    let index = index_in(&dir);
    index
        .with(|db| {
            // Two long credit lists that differ only after the point where the file name is cut.
            let crowd: Vec<(i64, String)> = (0..40)
                .map(|i| (i + 10, format!("Artist Number {i:02}")))
                .collect();
            let with = |last: &str| {
                let mut v = crowd.clone();
                v.push((99, last.to_string()));
                v
            };
            let a = upsert_song(db, &with("Zed"), "Same Title", None, 1)?;
            let b = upsert_song(db, &with("Zee"), "Same Title", None, 2)?;
            assert_ne!(a, b);
            let rows = db.query("SELECT filename FROM songs ORDER BY id", &[])?;
            let (first, second) = (
                songbook_service::index::str_at(&rows.rows[0], 0),
                songbook_service::index::str_at(&rows.rows[1], 0),
            );
            assert!(
                first.ends_with(" - Same Title") && first.len() <= 200,
                "{first}"
            );
            assert!(
                second.contains(" - Same Title [") && second.ends_with(']') && second.len() <= 200,
                "{second}"
            );
            Ok(())
        })
        .unwrap();
}

#[quench_test::test]
fn pruning_drops_songs_on_no_album_and_the_artists_only_they_had() {
    let dir = Scratch::new();
    let index = index_in(&dir);
    catalogue(&index);
    index
        .with(|db| {
            let a2 = songbook_service::index::int_at(
                &db.query("SELECT id FROM albums WHERE deezer_id = 2", &[])?
                    .rows[0],
                0,
            );
            songbook_service::index::songs::drop_album(db, a2)?;
            let (songs, artists) = prune(db)?;
            // "Never Leave Me Alone" was only on album 2; Snoop Dogg was only on it
            assert_eq!((songs, artists), (1, 1));
            let left = db.query("SELECT COUNT(*) FROM songs", &[])?.rows[0][0].clone();
            assert_eq!(left, quench_sqlite::Value::Integer(1));
            // the surviving song keeps both Deezer track ids it was found under
            assert_eq!(
                db.query("SELECT COUNT(*) FROM song_track", &[])?.rows[0][0],
                quench_sqlite::Value::Integer(2)
            );
            Ok(())
        })
        .unwrap();
}

#[quench_test::test]
fn tags_follow_the_category_of_the_followed_artists_and_the_genres_of_the_albums() {
    let dir = Scratch::new();
    let index = index_in(&dir);
    catalogue(&index);
    let mut cmap = HashMap::new();
    cmap.insert("Nate Dogg".to_string(), "American Rap".to_string());
    index
        .with(|db| {
            let n = retag(db, &cmap)?;
            assert!(n > 0);
            let tags = db.query("SELECT kind, name FROM tags ORDER BY kind, name", &[])?;
            let tags: Vec<(String, String)> = tags
                .rows
                .iter()
                .map(|r| {
                    (
                        songbook_service::index::str_at(r, 0),
                        songbook_service::index::str_at(r, 1),
                    )
                })
                .collect();
            assert_eq!(
                tags,
                vec![
                    ("category".to_string(), "American Rap".to_string()),
                    ("genre".to_string(), "R&B".to_string()),
                    ("genre".to_string(), "Rap/Hip Hop".to_string()),
                ]
            );
            // a custom tag survives a rebuild
            db.execute(
                "INSERT INTO tags(kind, name) VALUES ('custom', 'favourite')",
                &[],
            )?;
            retag(db, &cmap)?;
            assert_eq!(
                db.query("SELECT COUNT(*) FROM tags WHERE kind = 'custom'", &[])?
                    .rows[0][0],
                quench_sqlite::Value::Integer(1)
            );
            // an artist with no category gets Uncategorized
            retag(db, &HashMap::new())?;
            let uncategorized = db.query(
                "SELECT COUNT(*) FROM tags WHERE name = 'Uncategorized'",
                &[],
            )?;
            assert_eq!(uncategorized.rows[0][0], quench_sqlite::Value::Integer(1));
            Ok(())
        })
        .unwrap();
}

#[quench_test::test]
fn the_library_lists_albums_and_filters_tracks() {
    let dir = Scratch::new();
    let index = index_in(&dir);
    let (s1, _) = catalogue(&index);
    index
        .with(|db| {
            follow::add_category(db, "American Rap")?.unwrap();
            follow::add_follow(db, "Nate Dogg", "", "American Rap")?.unwrap();
            let cmap = follow::category_map(db)?;
            retag(db, &cmap)?;

            let all = browse::browse(db, &Filter::default())?;
            // three tracks listed, but one song is on two albums: two songs
            assert_eq!((all.total, all.linked, all.shown), (2, 0, 3));
            assert_eq!(all.albums.len(), 2);
            assert_eq!(all.albums[0].album, "Ghetto Preacher");
            assert_eq!(all.albums[1].tracks[1].artists, "Nate Dogg, Snoop Dogg");

            let by_text = browse::browse(
                db,
                &Filter {
                    q: "snoop".into(),
                    ..Filter::default()
                },
            )?;
            assert_eq!(by_text.total, 1);
            let by_genre = browse::browse(
                db,
                &Filter {
                    genre: "R&B".into(),
                    ..Filter::default()
                },
            )?;
            assert_eq!(
                (by_genre.total, by_genre.shown),
                (2, 3),
                "the remastered song is tagged R&B through its second album, and is listed on both: two songs, three tracks"
            );
            let by_category = browse::browse(
                db,
                &Filter {
                    category: "American Rap".into(),
                    artist: "Nate Dogg".into(),
                    ..Filter::default()
                },
            )?;
            assert_eq!(by_category.total, 2);
            assert_eq!(
                browse::browse(
                    db,
                    &Filter {
                        artist: "Nobody".into(),
                        ..Filter::default()
                    }
                )?
                .total,
                0
            );

            assert_eq!(browse::set_link(db, s1, "https://youtu.be/x")?.unwrap(), 1);
            let linked = browse::browse(
                db,
                &Filter {
                    links: "has".into(),
                    ..Filter::default()
                },
            )?;
            assert_eq!((linked.total, linked.linked), (1, 1), "one song, on two albums");
            assert!(linked.albums[0].tracks[0].manual && !linked.albums[0].tracks[0].official);
            assert_eq!(
                browse::browse(
                    db,
                    &Filter {
                        links: "missing".into(),
                        ..Filter::default()
                    }
                )?
                .total,
                1
            );
            assert_eq!(
                browse::browse(
                    db,
                    &Filter {
                        links: "unofficial".into(),
                        ..Filter::default()
                    }
                )?
                .total,
                1,
                "linked, but not official"
            );

            assert!(browse::set_link(db, s1, "ftp://nope").unwrap().is_err());
            assert!(browse::set_link(db, 9999, "").unwrap().is_err());
            browse::set_link(db, s1, "")?.unwrap();
            assert_eq!(
                browse::browse(
                    db,
                    &Filter {
                        links: "has".into(),
                        ..Filter::default()
                    }
                )?
                .total,
                0
            );

            let state = browse::state(db)?;
            assert_eq!(state.artists.len(), 1);
            assert_eq!(
                (state.artists[0].tracks, state.artists[0].category.as_str()),
                (2, "American Rap")
            );
            assert_eq!(state.genres, vec!["R&B", "Rap/Hip Hop"]);
            // The side panel and the library page count the same thing for an artist.
            let selected = browse::browse(
                db,
                &Filter {
                    artist: "Nate Dogg".into(),
                    ..Filter::default()
                },
            )?;
            assert_eq!(state.artists[0].tracks, selected.total);
            Ok(())
        })
        .unwrap();
}

#[quench_test::test]
fn the_statistics_count_the_library_and_what_is_on_disk() {
    let dir = Scratch::new();
    let index = index_in(&dir);
    let (s1, _) = catalogue(&index);
    let audio = dir.path().join("audio");
    std::fs::create_dir_all(&audio).unwrap();
    index
        .with(|db| {
            follow::add_follow(db, "Nate Dogg", "", "")?.unwrap();
            browse::set_link(db, s1, "https://youtu.be/x")?.unwrap();
            let snapshot = browse::Snapshot::load(db)?;
            // nothing downloaded yet: both songs are missing, one of them has a link
            let none = snapshot.stats(&songbook_service::files::file_index(&audio));
            assert_eq!(
                (none.artists, none.credited, none.albums, none.songs),
                (1, 2, 2, 2)
            );
            assert_eq!(
                (none.downloaded, none.missing, none.missing_linked),
                (0, 2, 1)
            );
            // a file of the first song, in any format, and a half-done one of the second, which does not count
            let name = db
                .query(
                    "SELECT filename FROM songs WHERE id = ?",
                    &[songbook_service::index::int(s1)],
                )?
                .rows[0][0]
                .as_text()
                .unwrap()
                .to_string();
            std::fs::write(audio.join(format!("{name}.webm")), b"x").unwrap();
            std::fs::write(
                audio.join("Nate Dogg, Snoop Dogg - Never Leave Me Alone.mp3.part"),
                b"x",
            )
            .unwrap();
            let some = snapshot.stats(&songbook_service::files::file_index(&audio));
            assert_eq!(
                (some.downloaded, some.missing, some.missing_linked),
                (1, 1, 0)
            );
            Ok(())
        })
        .unwrap();
}

#[quench_test::test]
fn a_blacklisted_album_leaves_with_its_other_editions_and_its_songs() {
    let dir = Scratch::new();
    let index = index_in(&dir);
    catalogue(&index);
    index
        .with(|db| {
            follow::add_follow(db, "Nate Dogg", "", "")?.unwrap();
            assert!(
                browse::blacklist_album(db, "Nate Dogg", 12345)?.is_err(),
                "unknown album"
            );
            let removed = browse::blacklist_album(db, "Nate Dogg", 2)?.unwrap();
            assert_eq!(removed, 2);
            assert!(
                follow::blacklist(db)?
                    .contains(&("natedogg".to_string(), "greatesthits".to_string()))
            );
            let left = browse::browse(db, &Filter::default())?;
            assert_eq!(left.total, 1);
            Ok(())
        })
        .unwrap();
}

#[quench_test::test]
fn the_blacklist_is_listed_added_by_name_changed_and_let_go_of() {
    let dir = Scratch::new();
    let index = index_in(&dir);
    catalogue(&index);
    index
        .with(|db| {
            follow::add_follow(db, "Nate Dogg", "", "")?.unwrap();
            assert!(
                browse::blacklist_save(db, 0, "  ", "x")?.is_err(),
                "both are needed"
            );
            // By name: the album in the index leaves at once, and it is on the list.
            let removed = browse::blacklist_save(db, 0, "Nate Dogg", "Greatest Hits")?.unwrap();
            assert_eq!(removed, 2);
            assert_eq!(browse::browse(db, &Filter::default())?.total, 1);
            let listed = follow::blacklist_entries(db)?;
            assert_eq!(listed.len(), 1);
            assert_eq!(
                (listed[0].1.as_str(), listed[0].2.as_str()),
                ("Nate Dogg", "Greatest Hits")
            );
            assert!(
                browse::blacklist_save(db, 0, "nate dogg", "GREATEST HITS")?.is_err(),
                "the same album is not listed twice"
            );
            // Changed: the entry takes the new title (nothing in the index has it).
            let id = listed[0].0;
            browse::blacklist_save(db, id, "Nate Dogg", "Another Record")?.unwrap();
            let listed = follow::blacklist_entries(db)?;
            assert_eq!(listed.len(), 1);
            assert_eq!(listed[0].2, "Another Record");
            assert!(
                browse::blacklist_save(db, 999, "A", "B")?.is_err(),
                "unknown entry"
            );
            // Let go of: the entry goes (the next sync brings the album back).
            assert!(follow::blacklist_remove(db, id)?);
            assert!(!follow::blacklist_remove(db, id)?);
            assert_eq!(
                follow::blacklist_entries(db)?,
                [] as [(i64, String, String); 0]
            );
            Ok(())
        })
        .unwrap();
}

#[quench_test::test]
fn followed_artists_and_categories_are_managed_in_the_index() {
    let dir = Scratch::new();
    let index = index_in(&dir);
    index
        .with(|db| {
            assert!(follow::add_follow(db, "  ", "", "")?.is_err());
            assert!(follow::add_follow(db, "Nas", "", "Nope")?.is_err());
            assert!(follow::add_category(db, "Uncategorized")?.is_err());
            assert!(follow::add_category(db, "Polish Rap")?.unwrap());
            assert!(!follow::add_category(db, "polish rap")?.unwrap());
            assert!(follow::add_follow(db, "Nas", "id 73", "polish rap")?.unwrap());
            assert!(
                !follow::add_follow(db, "NAS", "", "")?.unwrap(),
                "already followed"
            );
            let f = follow::follows(db)?;
            assert_eq!(
                (
                    f[0].name.as_str(),
                    f[0].pin.as_str(),
                    f[0].category.as_str()
                ),
                ("Nas", "73", "Polish Rap")
            );
            follow::set_category(db, "nas", "")?.unwrap();
            assert_eq!(follow::follows(db)?[0].category, "");
            assert!(follow::set_category(db, "Who", "")?.is_err());
            assert_eq!(follow::clean(" a | b # c\nd "), "a   b   c d");
            follow::remove_follow(db, "NAS")?;
            assert_eq!(
                follow::follows(db)?,
                [] as [songbook_service::index::follow::Follow; 0]
            );
            Ok(())
        })
        .unwrap();
}

#[quench_test::test]
fn a_category_is_renamed_with_the_artists_in_it() {
    let dir = Scratch::new();
    let index = index_in(&dir);
    index
        .with(|db| {
            follow::add_category(db, "Polish Rap")?.unwrap();
            follow::add_category(db, "Latino")?.unwrap();
            follow::add_follow(db, "Kartky", "", "Polish Rap")?.unwrap();
            follow::add_follow(db, "Nas", "", "Latino")?.unwrap();
            follow::rename_category(db, "polish rap", "Rap PL")?.unwrap();
            assert_eq!(follow::categories(db)?, ["Rap PL", "Latino"], "same place");
            let f = follow::follows(db)?;
            assert_eq!(
                (f[0].category.as_str(), f[1].category.as_str()),
                ("Rap PL", "Latino")
            );
            // a case change is fine; another category's name, an empty one and Uncategorized are not
            follow::rename_category(db, "Rap PL", "rap pl")?.unwrap();
            assert_eq!(follow::categories(db)?[0], "rap pl");
            assert!(follow::rename_category(db, "rap pl", "LATINO")?.is_err());
            assert!(follow::rename_category(db, "rap pl", "  ")?.is_err());
            assert!(follow::rename_category(db, "rap pl", "Uncategorized")?.is_err());
            assert!(follow::rename_category(db, "Nope", "X")?.is_err());
            assert!(follow::rename_category(db, "", "X")?.is_err());
            Ok(())
        })
        .unwrap();
}

#[quench_test::test]
fn the_old_keys_are_recomputed_once_and_the_text_files_imported_once() {
    let dir = Scratch::new();
    let meta = dir.path().join("meta");
    std::fs::create_dir_all(&meta).unwrap();
    std::fs::write(meta.join("categories.txt"), "Polish Rap\nAmerican Rap\n").unwrap();
    std::fs::write(
        meta.join("artists.txt"),
        "# comment\nNas |  | American Rap\nGibbs | 12345 | Polish Rap  # note\nKartky\n",
    )
    .unwrap();
    std::fs::write(meta.join("blacklist.txt"), "ReTo | THE LAST BRAP\n").unwrap();
    std::fs::write(meta.join("artist_matches.csv"), "name_in_artists_txt,deezer_name,deezer_id,fans,note\nNas,Nas,73,1216342,\nKartky,,,,NO MATCH\n").unwrap();
    std::fs::write(
        meta.join("suggestions.csv"),
        "artist,deezer_id,fans,related_to_count,related_to,scope\nJay-Z,1,100,3,\"Nas; Gibbs\",\n",
    )
    .unwrap();
    // a database from the Python helper: ASCII keys, so "Ż" and "Ó" collided as "" there
    {
        let mut db = quench_sqlite::Connection::open_or_create(meta.join("library.db")).unwrap();
        db.execute(songbook_service::index::SCHEMA, &[]).unwrap();
        db.execute(
            "INSERT INTO artists(id, deezer_id, name) VALUES (1, 1, 'Gibbs')",
            &[],
        )
        .unwrap();
        for (id, title) in [(1, "Ż"), (2, "Ó")] {
            db.execute(
                "INSERT INTO songs(id, key, title, filename) VALUES (?, ?, ?, ?)",
                &[
                    int(id),
                    text("gibbs||"),
                    text(title),
                    text(&format!("Gibbs - {title}")),
                ],
            )
            .ok();
            db.execute(
                "INSERT OR IGNORE INTO song_artist(song_id, artist_id, position) VALUES (?, 1, 0)",
                &[int(id)],
            )
            .unwrap();
        }
    }
    let index = Index::open(&meta.join("library.db"), &meta).unwrap();
    index
        .with(|db| {
            // the second song could not be inserted with the old key; the first is rekeyed
            let keys = db.query("SELECT key FROM songs ORDER BY id", &[])?;
            assert_eq!(
                songbook_service::index::str_at(&keys.rows[0], 0),
                "gibbs||ż"
            );
            let f = follow::follows(db)?;
            assert_eq!(
                f.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(),
                vec!["Nas", "Gibbs", "Kartky"]
            );
            assert_eq!(
                (f[1].pin.as_str(), f[1].category.as_str()),
                ("12345", "Polish Rap")
            );
            assert_eq!(f[0].note.as_deref(), Some(""));
            assert_eq!(f[2].note.as_deref(), Some("NO MATCH"));
            assert_eq!(follow::categories(db)?, vec!["Polish Rap", "American Rap"]);
            assert_eq!(follow::suggestions(db)?[0].related_to, "Nas; Gibbs");
            assert_eq!(follow::blacklist(db)?.len(), 1);
            Ok(())
        })
        .unwrap();
    drop(index);
    // a second start does not import again
    std::fs::write(meta.join("artists.txt"), "Someone Else\n").unwrap();
    let index = Index::open(&meta.join("library.db"), &meta).unwrap();
    assert_eq!(index.with(follow::follows).unwrap().len(), 3);
}

#[quench_test::test]
fn top_and_suggested_list_songs_by_popularity() {
    let dir = Scratch::new();
    let index = index_in(&dir);
    catalogue(&index);
    index
        .with(|db| {
            let ids: Vec<i64> = db
                .query("SELECT id FROM songs ORDER BY id", &[])?
                .rows
                .iter()
                .map(|r| songbook_service::index::int_at(r, 0))
                .collect();
            assert_eq!(ids.len(), 2);
            let names: Vec<String> = db
                .query("SELECT filename FROM songs ORDER BY id", &[])?
                .rows
                .iter()
                .map(|r| songbook_service::index::str_at(r, 0))
                .collect();
            // the second song is the popular one
            db.execute(
                "UPDATE songs SET rank = 800000 WHERE id = ?",
                &[songbook_service::index::int(ids[1])],
            )?;
            db.execute(
                "UPDATE songs SET yt_views = 1000 WHERE id = ?",
                &[songbook_service::index::int(ids[0])],
            )?;
            let snapshot = browse::Snapshot::load(db)?;
            let top = snapshot.browse(&Filter {
                sort: "top".into(),
                ..Filter::default()
            });
            assert_eq!(top.albums.len(), 1);
            assert_eq!(top.albums[0].album, "");
            let order: Vec<i64> = top.albums[0].tracks.iter().map(|t| t.id).collect();
            assert_eq!(order, vec![ids[1], ids[0]]);
            assert!(top.albums[0].tracks[0].score > top.albums[0].tracks[1].score);

            let mut on_disk = HashMap::new();
            on_disk.insert(names[1].clone(), vec![format!("{}.mp3", names[1])]);
            let suggested = snapshot.browse_on_disk(
                &Filter {
                    sort: "suggested".into(),
                    ..Filter::default()
                },
                &on_disk,
            );
            let order: Vec<i64> = suggested.albums[0].tracks.iter().map(|t| t.id).collect();
            assert_eq!(order, vec![ids[0]]); // the popular one is already downloaded
            Ok(())
        })
        .unwrap();
}

#[quench_test::test]
fn top_artists_lists_the_most_followed_first_with_only_their_best_songs() {
    let dir = Scratch::new();
    let index = index_in(&dir);
    index
        .with(|db| {
            for (deezer, name, fans, rank_base) in [
                (1, "Small Act", "10", 900_000),
                (2, "Big Act", "5000", 100_000),
            ] {
                follow::add_follow(db, name, "", "")?.unwrap();
                follow::set_match(db, name, name, &deezer.to_string(), fans, "")?;
                let artist = upsert_artist(db, deezer, name, Some(name))?;
                let al = upsert_album(db, &album(deezer, &format!("{name} LP"), &["Pop"]), artist)?;
                // twelve songs, so the cap shows
                for n in 0..12 {
                    let s = upsert_song(
                        db,
                        &names(&[(deezer, name)]),
                        &format!("{name} song {n}"),
                        Some(200),
                        deezer * 100 + n,
                    )?;
                    link_album(db, s, al, n + 1)?;
                    db.execute(
                        "UPDATE songs SET rank = ? WHERE id = ?",
                        &[int(rank_base + n * 1000), int(s)],
                    )?;
                }
            }
            let snapshot = browse::Snapshot::load(db)?;
            let by = snapshot.browse(&Filter {
                sort: "artists".into(),
                ..Filter::default()
            });
            let groups: Vec<&str> = by.albums.iter().map(|a| a.album.as_str()).collect();
            // Big Act has fewer-ranked songs but more fans, so it comes first.
            assert_eq!(groups, ["Big Act", "Small Act"]);
            assert!(by.albums.iter().all(|a| a.id == 0 && a.artist.is_empty()));
            assert!(
                by.albums
                    .iter()
                    .all(|a| a.tracks.len() == browse::BEST_PER_ARTIST)
            );
            let first: Vec<&str> = by.albums[0]
                .tracks
                .iter()
                .map(|t| t.title.as_str())
                .collect();
            assert_eq!(first[0], "Big Act song 11", "best song first");
            assert_eq!(first[9], "Big Act song 2");
            Ok(())
        })
        .unwrap();
}

#[quench_test::test]
fn suggestions_keep_their_best_songs() {
    let dir = Scratch::new();
    let index = index_in(&dir);
    index
        .with(|db| {
            let best = vec![
                songbook_core::TopTrack {
                    title: "Regulate".into(),
                    album: "G Funk".into(),
                    score: 71,
                },
                songbook_core::TopTrack {
                    title: "Second".into(),
                    album: String::new(),
                    score: 40,
                },
            ];
            let list = vec![songbook_core::Suggestion {
                artist: "Warren G".into(),
                deezer_id: "712".into(),
                fans: "1".into(),
                related_to_count: 1,
                related_to: "Nate Dogg".into(),
                scope: String::new(),
                best,
            }];
            follow::set_suggestions(db, &list)?;
            assert_eq!(follow::suggestions(db)?, list);
            follow::set_suggestions(db, &[])?;
            assert_eq!(
                follow::suggestions(db)?,
                [] as [songbook_core::Suggestion; 0]
            );
            let left = db.query("SELECT COUNT(*) FROM suggestion_tracks", &[])?;
            assert_eq!(left.rows[0][0], quench_sqlite::Value::Integer(0));
            Ok(())
        })
        .unwrap();
}
