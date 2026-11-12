use crate::support::*;
use songbook_service::app::App;
use songbook_service::index::follow;
use songbook_service::sync::{clean_name, credit_parts, split_combined};
use std::collections::HashMap;
use std::time::Duration;

async fn sync(app: &App, singles: bool) -> (Option<String>, String) {
    let worker = app.clone();
    assert!(app.jobs.start("sync", move |log| async move {
        songbook_service::sync::run(&worker, singles, &log).await
    }));
    for _ in 0..500 {
        if !app.jobs.running() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let job = app.jobs.read(0);
    (job.error, job.log)
}

const ANSWERS: [(&str, &str); 11] = [
    (
        "search/artist",
        r#"{"data": [{"id": 1, "name": "Nas", "nb_fan": 5}]}"#,
    ),
    (
        "artist/1/albums",
        r#"{"data": [
            {"id": 10, "title": "Illmatic", "record_type": "album", "release_date": "1994-04-19"},
            {"id": 11, "title": "Illmatic (Deluxe Edition)", "record_type": "album", "release_date": "2014-01-01"},
            {"id": 12, "title": "A Single", "record_type": "single", "release_date": "1995-01-01"}]}"#,
    ),
    (
        "album/10",
        r#"{"id": 10, "title": "Illmatic", "release_date": "1994-04-19", "record_type": "album", "genres": {"data": [{"name": "Rap/Hip Hop"}]}}"#,
    ),
    (
        "album/10/tracks",
        r#"{"data": [{"id": 101, "title": "N.Y. State of Mind", "duration": 294, "artist": {"id": 1, "name": "Nas"}},
                      {"id": 102, "title": "Life's a Bitch", "duration": 220, "artist": {"id": 1, "name": "Nas"}},
                      {"id": 103, "title": "Mystery", "duration": 100, "artist": {"id": 3, "name": "Someone"}}]}"#,
    ),
    (
        "track/101",
        r#"{"id": 101, "title": "N.Y. State of Mind", "duration": 294, "contributors": [{"id": 1, "name": "Nas"}]}"#,
    ),
    (
        "track/102",
        r#"{"id": 102, "title": "Life's a Bitch (feat. AZ)", "duration": 221, "contributors": [{"id": 4, "name": "AZ"}, {"id": 1, "name": "Nas"}]}"#,
    ),
    ("track/103", "{}"),
    (
        "album/12",
        r#"{"id": 12, "title": "A Single", "release_date": "1995-01-01", "record_type": "single", "genres": {"data": []}}"#,
    ),
    (
        "album/12/tracks",
        r#"{"data": [{"id": 121, "title": "Single Track", "artist": {"id": 1, "name": "Nas"}}]}"#,
    ),
    (
        "track/121",
        r#"{"id": 121, "title": "Single Track", "contributors": [{"id": 1, "name": "Nas"}, {"id": 5, "name": "Nas featuring AZ"}]}"#,
    ),
    ("artist/1/related", r#"{"data": []}"#),
];

#[quench_test::test]
fn invisible_marks_and_their_mis_decoded_form_are_not_part_of_a_name() {
    assert_eq!(
        clean_name("\u{e2}\u{20ac}\u{17d}Ol' Dirty Bastard"),
        "Ol' Dirty Bastard"
    );
    assert_eq!(
        clean_name("\u{200e}Ol' Dirty Bastard "),
        "Ol' Dirty Bastard"
    );
    assert_eq!(clean_name("Pięć Dwa Dębiec"), "Pięć Dwa Dębiec");
}

#[quench_test::test]
fn combined_credits_are_split_only_when_every_part_is_known() {
    assert_eq!(
        credit_parts("Nate Dogg featuring Snoop Dogg"),
        vec!["Nate Dogg", "Snoop Dogg"]
    );
    assert_eq!(credit_parts("2Pac, Nate Dogg"), vec!["2Pac", "Nate Dogg"]);
    assert_eq!(
        credit_parts("Earth, Wind & Fire"),
        vec!["Earth", "Wind", "Fire"]
    );
    assert_eq!(
        credit_parts("A feat. B ft. C and D with E"),
        vec!["A", "B", "C", "D", "E"]
    );
    assert_eq!(
        credit_parts("Sandy Lee"),
        vec!["Sandy Lee"],
        "'and' inside a word does not split"
    );
    assert_eq!(credit_parts("Frankie"), vec!["Frankie"]);

    let known: HashMap<String, (i64, String)> = [
        ("natedogg".to_string(), (1, "Nate Dogg".to_string())),
        ("snoopdogg".to_string(), (2, "Snoop Dogg".to_string())),
    ]
    .into_iter()
    .collect();
    let people = vec![
        (9, "Nate Dogg featuring Snoop Dogg".to_string()),
        (1, "Nate Dogg".to_string()),
        (7, "Earth, Wind & Fire".to_string()),
    ];
    let out = split_combined(&people, &known);
    assert_eq!(
        out,
        vec![
            (1, "Nate Dogg".to_string()),
            (2, "Snoop Dogg".to_string()),
            (7, "Earth, Wind & Fire".to_string())
        ]
    );
}

#[quench_test::test]
async fn a_sync_writes_the_discography_of_a_followed_artist_into_the_index() {
    if !have_database() {
        return;
    }
    let dir = Scratch::new();
    let app = app_with(&dir, &ANSWERS).await;
    app.index
        .run(async |db| {
            follow::add_category(db, "American Rap").await?.unwrap();
            follow::add_follow(db, "Nas", "", "American Rap")
                .await?
                .unwrap();
            follow::add_follow(db, "Nobody Known", "", "")
                .await?
                .unwrap();
            Ok(())
        })
        .await
        .unwrap();
    let (error, log) = sync(&app, false).await;
    assert_eq!(error, None, "{log}");
    assert!(
        log.contains("Nas -> Nas (5 fans), 1 releases"),
        "the deluxe edition is the same album, the single is not wanted: {log}"
    );
    assert!(log.contains("NO MATCH: Nobody Known"), "{log}");
    app.index
        .run(async |db| {
            let files = db
                .query(
                    "SELECT filename, duration FROM songs ORDER BY filename",
                    &[],
                )
                .await?;
            let files: Vec<(String, i64)> = files
                .rows
                .iter()
                .map(|r| {
                    (
                        songbook_service::index::str_at(r, 0),
                        songbook_service::index::int_at(r, 1),
                    )
                })
                .collect();
            assert_eq!(
                files,
                vec![
                    ("AZ, Nas - Life's a Bitch".to_string(), 221),
                    ("Nas - N.Y. State of Mind".to_string(), 294),
                    ("Someone - Mystery".to_string(), 100),
                ]
            );
            let f = follow::follows(db).await?;
            assert_eq!(
                (
                    f[0].deezer_id.as_str(),
                    f[0].fans.as_str(),
                    f[0].note.as_deref()
                ),
                ("1", "5", Some(""))
            );
            assert_eq!(f[1].note.as_deref(), Some("NO MATCH"));
            let tags = db
                .query("SELECT kind, name FROM tags ORDER BY kind, name", &[])
                .await?
                .rows
                .len();
            assert_eq!(tags, 2, "American Rap and Rap/Hip Hop");
            Ok(())
        })
        .await
        .unwrap();

    // a second sync changes nothing, and a song cannot be duplicated
    let (error, _) = sync(&app, false).await;
    assert_eq!(error, None);
    assert_eq!(
        app.index
            .run(async |db| Ok(
                db.query("SELECT COUNT(*) FROM songs", &[]).await?.rows[0][0].clone()
            ))
            .await
            .unwrap(),
        songbook_service::index::Value::Integer(3)
    );

    // with singles, the single comes too, and its combined credit is split into the artists we know
    let (error, _) = sync(&app, true).await;
    assert_eq!(error, None);
    let names = app
        .index
        .run(async |db| {
            Ok(db
                .query(
                    "SELECT filename FROM songs WHERE title = 'Single Track'",
                    &[],
                )
                .await?
                .rows
                .len())
        })
        .await
        .unwrap();
    assert_eq!(names, 1);
    let credit = app
        .index
        .run(async |db| {
            let id = db
                .query("SELECT id FROM songs WHERE title = 'Single Track'", &[])
                .await?
                .rows[0][0]
                .as_integer()
                .unwrap();
            songbook_service::index::songs::artists_of(db, id).await
        })
        .await
        .unwrap();
    assert_eq!(
        credit,
        vec!["Nas".to_string(), "AZ".to_string()],
        "'Nas featuring AZ' is Nas and AZ, both known from the other tracks"
    );
}

#[quench_test::test]
async fn an_unfollowed_artist_and_a_blacklisted_album_leave_at_the_next_sync() {
    if !have_database() {
        return;
    }
    let dir = Scratch::new();
    let app = app_with(&dir, &ANSWERS).await;
    app.index
        .run(async |db| {
            follow::add_follow(db, "Nas", "", "").await?.unwrap();
            Ok(())
        })
        .await
        .unwrap();
    assert_eq!(sync(&app, false).await.0, None);
    assert_eq!(
        app.index
            .run(async |db| Ok(
                db.query("SELECT COUNT(*) FROM songs", &[]).await?.rows[0][0].as_integer()
            ))
            .await
            .unwrap(),
        Some(3)
    );

    app.index
        .run(async |db| {
            follow::remove_follow(db, "Nas").await?;
            Ok(())
        })
        .await
        .unwrap();
    assert_eq!(sync(&app, false).await.0, None);
    app.index
        .run(async |db| {
            assert_eq!(
                db.query("SELECT COUNT(*) FROM songs", &[]).await?.rows[0][0].as_integer(),
                Some(0)
            );
            assert_eq!(
                db.query("SELECT COUNT(*) FROM albums", &[]).await?.rows[0][0].as_integer(),
                Some(0)
            );
            Ok(())
        })
        .await
        .unwrap();
}

#[quench_test::test]
async fn similar_saves_the_best_songs_of_each_suggested_artist() {
    use songbook_service::index::follow;
    if !have_database() {
        return;
    }
    let dir = Scratch::new();
    let app = app_with(
        &dir,
        &[
            (
                "artist/1/related",
                r#"{"data": [{"id": 712, "name": "Warren G", "nb_fan": 99}]}"#,
            ),
            (
                "artist/712/top",
                r#"{"data": [{"id": 9, "title": "Regulate", "rank": 700000, "album": {"title": "G Funk"}}]}"#,
            ),
        ],
    ).await;
    app.index
        .run(async |db| {
            follow::add_follow(db, "Nas", "", "").await?.unwrap();
            follow::set_match(db, "Nas", "Nas", "1", "1", "").await
        })
        .await
        .unwrap();
    let worker = app.clone();
    assert!(app.jobs.start("similar", move |log| async move {
        songbook_service::similar::run(&worker, 10, 10, "", &log).await
    }));
    for _ in 0..200 {
        if !app.jobs.running() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let job = app.jobs.read(0);
    assert_eq!(job.error, None, "{}", job.log);
    let list = app
        .index
        .run(async |db| follow::suggestions(db).await)
        .await
        .unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].artist, "Warren G");
    assert_eq!(list[0].best.len(), 1);
    assert_eq!(
        (
            list[0].best[0].title.as_str(),
            list[0].best[0].album.as_str(),
            list[0].best[0].score
        ),
        ("Regulate", "G Funk", 70)
    );
}

#[quench_test::test]
async fn purge_deletes_only_the_files_of_blacklisted_albums_that_the_library_does_not_keep() {
    use songbook_service::index::browse;
    if !have_database() {
        return;
    }
    let dir = Scratch::new();
    let app = app_with(&dir, &ANSWERS).await;
    app.index
        .run(async |db| {
            follow::add_follow(db, "Nas", "", "").await?.unwrap();
            Ok(())
        })
        .await
        .unwrap();
    let (err, log) = sync(&app, false).await;
    assert_eq!(err, None, "{log}");
    // Files: Illmatic's three tracks, one kept-album song, a hand-made file.
    std::fs::create_dir_all(dir.path().join("data")).unwrap();
    let data = dir.path().join("data");
    let names: Vec<String> = app
        .index
        .run(async |db| {
            Ok(db
                .query("SELECT filename FROM songs ORDER BY id", &[])
                .await?
                .rows
                .iter()
                .map(|r| songbook_service::index::str_at(r, 0))
                .collect())
        })
        .await
        .unwrap();
    for n in &names {
        std::fs::write(data.join(format!("{n}.mp3")), b"audio").unwrap();
    }
    std::fs::write(data.join("My own recording.mp3"), b"mine").unwrap();
    // Blacklist Illmatic: its songs leave the index but the files stay.
    app.index
        .run(async |db| {
            browse::blacklist_album(db, "Nas", 10)
                .await
                .map(|r| r.unwrap())
        })
        .await
        .unwrap();
    assert_eq!(
        app.index
            .run(async |db| Ok(
                db.query("SELECT COUNT(*) FROM songs", &[]).await?.rows[0][0].as_integer()
            ))
            .await
            .unwrap(),
        Some(0),
        "the blacklist removed the songs"
    );
    purge(&app, true).await;
    let preview = app.jobs.read(0);
    assert_eq!(preview.error, None, "{}", preview.log);
    assert!(preview.log.contains("would delete"), "{}", preview.log);
    let count = |dir: &std::path::Path| std::fs::read_dir(dir).unwrap().count();
    let before = count(&data);
    assert_eq!(before, names.len() + 1, "a preview deletes nothing");

    purge(&app, false).await;
    let done = app.jobs.read(0);
    assert_eq!(done.error, None, "{}", done.log);
    assert!(data.join("My own recording.mp3").exists(), "{}", done.log);
    assert_eq!(
        count(&data),
        1,
        "only the hand-made file is left: {}",
        done.log
    );
}

#[quench_test::test]
fn a_link_matches_when_the_video_title_holds_the_song_title() {
    use songbook_service::fixlinks::matches;
    assert!(matches(
        "N.Y. State of Mind",
        "Nas - N.Y. State Of Mind (Official Audio)"
    ));
    assert!(matches("Hysteria (feat. X)", "Muse - Hysteria"));
    assert!(matches("Pięć", "PIĘĆ - live"), "case is ignored");
    assert!(!matches("Uprising", "Some Other Song"));
    assert!(
        matches("", "anything"),
        "a song with no title is never in doubt"
    );
}

#[quench_test::test]
async fn fix_links_preview_lists_the_mismatched_automatic_links_and_changes_nothing() {
    use songbook_service::fixlinks;
    if !have_database() {
        return;
    }
    let dir = Scratch::new();
    let app = app_with(&dir, &ANSWERS).await;
    app.index
        .run(async |db| {
            follow::add_follow(db, "Nas", "", "").await?.unwrap();
            Ok(())
        })
        .await
        .unwrap();
    let (err, log) = sync(&app, false).await;
    assert_eq!(err, None, "{log}");
    let songs: Vec<(i64, String, String)> = app
        .index
        .run(async |db| {
            Ok(db
                .query("SELECT id, title, filename FROM songs ORDER BY id", &[])
                .await?
                .rows
                .iter()
                .map(|r| {
                    (
                        songbook_service::index::int_at(r, 0),
                        songbook_service::index::str_at(r, 1),
                        songbook_service::index::str_at(r, 2),
                    )
                })
                .collect())
        })
        .await
        .unwrap();
    assert!(songs.len() >= 3);
    let (right, wrong, manual) = (&songs[0], &songs[1], &songs[2]);
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(data.join(format!("{}.mp3", wrong.2)), b"audio").unwrap();
    app.index
        .run(async |db| {
            for (id, video, manual) in [
                (right.0, format!("Nas - {} (Official)", right.1), 0),
                (wrong.0, "A Completely Different Song".to_string(), 0),
                (manual.0, "Another Different Song".to_string(), 1),
            ] {
                db.execute(
                    "UPDATE songs SET yt_url = 'https://y/x', yt_title = ?, yt_manual = ? WHERE id = ?",
                    &[
                        songbook_service::index::text(&video),
                        songbook_service::index::int(manual),
                        songbook_service::index::int(id),
                    ],
                ).await?;
            }
            Ok(())
        }).await
        .unwrap();

    let found = app
        .index
        .run(async |db| fixlinks::suspects(db, "").await)
        .await
        .unwrap();
    assert_eq!(
        found.iter().map(|s| s.id).collect::<Vec<_>>(),
        vec![wrong.0],
        "only the automatic link that does not match"
    );

    let worker = app.clone();
    assert!(app.jobs.start("fix-links", move |log| async move {
        let job = songbook_core::JobStart {
            name: "fix-links".into(),
            dry_run: true,
            ..Default::default()
        };
        fixlinks::run(&worker, &job, &log).await
    }));
    wait(&app).await;
    let preview = app.jobs.read(0);
    assert_eq!(preview.error, None, "{}", preview.log);
    assert!(
        preview.log.contains("1 songs whose link does not match"),
        "{}",
        preview.log
    );
    assert!(
        data.join(format!("{}.mp3", wrong.2)).exists(),
        "a preview moves nothing"
    );
    assert_eq!(
        app.index
            .run(async |db| fixlinks::suspects(db, "").await)
            .await
            .unwrap()
            .len(),
        1
    );

    // The clearing step: the link is gone, so find links looks at the song again.
    app.index
        .run(async |db| fixlinks::clear(db, &[wrong.0]).await)
        .await
        .unwrap();
    assert_eq!(
        app.index
            .run(async |db| fixlinks::suspects(db, "").await)
            .await
            .unwrap(),
        vec![]
    );
    let status: Option<String> = app
        .index
        .run(async |db| {
            Ok(songbook_service::index::opt_str_at(
                &db.query(
                    "SELECT yt_status FROM songs WHERE id = ?",
                    &[songbook_service::index::int(wrong.0)],
                )
                .await?
                .rows[0],
                0,
            ))
        })
        .await
        .unwrap();
    assert_eq!(status, None);
}

async fn purge(app: &App, dry: bool) {
    let worker = app.clone();
    assert!(app.jobs.start("purge", move |log| async move {
        songbook_service::purge::run(&worker, dry, &log).await
    }));
    wait(app).await;
}

async fn wait(app: &App) {
    for _ in 0..500 {
        if !app.jobs.running() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[quench_test::test]
async fn an_artists_genres_keep_only_its_own_albums_and_drop_the_others_at_the_next_sync() {
    if !have_database() {
        return;
    }
    let dir = Scratch::new();
    let app = app_with(&dir, &ANSWERS).await;
    app.index
        .run(async |db| {
            follow::add_follow(db, "Nas", "", "").await?.unwrap();
            Ok(())
        })
        .await
        .unwrap();
    let songs = async |app: &App| {
        app.index
            .run(async |db| {
                Ok(db.query("SELECT COUNT(*) FROM songs", &[]).await?.rows[0][0].as_integer())
            })
            .await
            .unwrap()
    };
    assert_eq!(sync(&app, false).await.0, None);
    assert_eq!(songs(&app).await, Some(3), "no filter keeps every album");

    // The profile mixes in someone else: only Rock is wanted, so Illmatic (Rap/Hip Hop) goes.
    app.index
        .run(async |db| {
            follow::set_genres(db, "Nas", " Rock ;; Punk ")
                .await
                .map(|r| r.unwrap())
        })
        .await
        .unwrap();
    let rows = app
        .index
        .run(async |db| follow::artist_rows(db).await)
        .await
        .unwrap();
    assert_eq!(rows[0].genres, "Rock; Punk");
    let (err, log) = sync(&app, false).await;
    assert_eq!(err, None);
    assert!(
        log.contains("skip Illmatic (Rap/Hip Hop): not in Rock; Punk"),
        "{log}"
    );
    assert_eq!(songs(&app).await, Some(0));

    // The right genre, in any case, brings it back.
    app.index
        .run(async |db| {
            follow::set_genres(db, "Nas", "rap/hip hop")
                .await
                .map(|r| r.unwrap())
        })
        .await
        .unwrap();
    assert_eq!(sync(&app, false).await.0, None);
    assert_eq!(songs(&app).await, Some(3));

    // Cleared, every album again; an unknown artist is refused.
    app.index
        .run(async |db| follow::set_genres(db, "Nas", "").await.map(|r| r.unwrap()))
        .await
        .unwrap();
    assert_eq!(
        app.index
            .run(async |db| follow::artist_rows(db).await)
            .await
            .unwrap()[0]
            .genres,
        ""
    );
    assert!(
        app.index
            .run(async |db| follow::set_genres(db, "Nobody", "Rock").await)
            .await
            .unwrap()
            .is_err()
    );
}
