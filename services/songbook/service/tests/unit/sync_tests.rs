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
    let dir = Scratch::new();
    let app = app_with(&dir, &ANSWERS);
    app.index
        .with(|db| {
            follow::add_category(db, "American Rap")?.unwrap();
            follow::add_follow(db, "Nas", "", "American Rap")?.unwrap();
            follow::add_follow(db, "Nobody Known", "", "")?.unwrap();
            Ok(())
        })
        .unwrap();
    let (error, log) = sync(&app, false).await;
    assert_eq!(error, None, "{log}");
    assert!(
        log.contains("Nas -> Nas (5 fans), 1 releases"),
        "the deluxe edition is the same album, the single is not wanted: {log}"
    );
    assert!(log.contains("NO MATCH: Nobody Known"), "{log}");
    app.index
        .with(|db| {
            let files = db.query(
                "SELECT filename, duration FROM songs ORDER BY filename",
                &[],
            )?;
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
            let f = follow::follows(db)?;
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
                .query("SELECT kind, name FROM tags ORDER BY kind, name", &[])?
                .rows
                .len();
            assert_eq!(tags, 2, "American Rap and Rap/Hip Hop");
            Ok(())
        })
        .unwrap();

    // a second sync changes nothing, and a song cannot be duplicated
    let (error, _) = sync(&app, false).await;
    assert_eq!(error, None);
    assert_eq!(
        app.index
            .with(|db| Ok(db.query("SELECT COUNT(*) FROM songs", &[])?.rows[0][0].clone()))
            .unwrap(),
        quench_sqlite::Value::Integer(3)
    );

    // with singles, the single comes too, and its combined credit is split into the artists we know
    let (error, _) = sync(&app, true).await;
    assert_eq!(error, None);
    let names = app
        .index
        .with(|db| {
            Ok(db
                .query(
                    "SELECT filename FROM songs WHERE title = 'Single Track'",
                    &[],
                )?
                .rows
                .len())
        })
        .unwrap();
    assert_eq!(names, 1);
    let credit = app
        .index
        .with(|db| {
            let id = db
                .query("SELECT id FROM songs WHERE title = 'Single Track'", &[])?
                .rows[0][0]
                .as_integer()
                .unwrap();
            songbook_service::index::songs::artists_of(db, id)
        })
        .unwrap();
    assert_eq!(
        credit,
        vec!["Nas".to_string(), "AZ".to_string()],
        "'Nas featuring AZ' is Nas and AZ, both known from the other tracks"
    );
}

#[quench_test::test]
async fn an_unfollowed_artist_and_a_blacklisted_album_leave_at_the_next_sync() {
    let dir = Scratch::new();
    let app = app_with(&dir, &ANSWERS);
    app.index
        .with(|db| {
            follow::add_follow(db, "Nas", "", "")?.unwrap();
            Ok(())
        })
        .unwrap();
    assert_eq!(sync(&app, false).await.0, None);
    assert_eq!(
        app.index
            .with(|db| Ok(db.query("SELECT COUNT(*) FROM songs", &[])?.rows[0][0].as_integer()))
            .unwrap(),
        Some(3)
    );

    app.index
        .with(|db| {
            follow::remove_follow(db, "Nas")?;
            Ok(())
        })
        .unwrap();
    assert_eq!(sync(&app, false).await.0, None);
    app.index
        .with(|db| {
            assert_eq!(
                db.query("SELECT COUNT(*) FROM songs", &[])?.rows[0][0].as_integer(),
                Some(0)
            );
            assert_eq!(
                db.query("SELECT COUNT(*) FROM albums", &[])?.rows[0][0].as_integer(),
                Some(0)
            );
            Ok(())
        })
        .unwrap();
}

#[quench_test::test]
async fn similar_saves_the_best_songs_of_each_suggested_artist() {
    use songbook_service::index::follow;
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
    );
    app.index
        .with(|db| {
            follow::add_follow(db, "Nas", "", "")?.unwrap();
            follow::set_match(db, "Nas", "Nas", "1", "1", "")
        })
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
    let list = app.index.with(follow::suggestions).unwrap();
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
