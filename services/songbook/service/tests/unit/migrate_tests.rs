use crate::support::*;
use songbook_service::index::{Db, str_at};
use songbook_service::migrate::{Options, parse_args, run};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const LIBRARY_SCHEMA: &str = include_str!("../legacy_sqlite_schema.sql");

/// The schema `meta/user.db` had.
const USER_SCHEMA: &str = "
CREATE TABLE playlists(id INTEGER PRIMARY KEY, owner TEXT NOT NULL, name TEXT NOT NULL, created INTEGER NOT NULL DEFAULT 0);
CREATE TABLE playlist_songs(playlist_id INTEGER NOT NULL, position INTEGER NOT NULL, song_id INTEGER NOT NULL, PRIMARY KEY(playlist_id, position));
CREATE TABLE plays(owner TEXT NOT NULL, song_id INTEGER NOT NULL, at INTEGER NOT NULL);
CREATE TABLE contexts(owner TEXT NOT NULL, key TEXT NOT NULL, kind TEXT NOT NULL, name TEXT NOT NULL, sub TEXT NOT NULL, art_kind TEXT NOT NULL,
  art_key TEXT NOT NULL, query TEXT NOT NULL, shuffle INTEGER NOT NULL, at INTEGER NOT NULL, PRIMARY KEY(owner, key));
CREATE TABLE resume(owner TEXT PRIMARY KEY, song_id INTEGER NOT NULL, position_ms INTEGER NOT NULL, queue TEXT NOT NULL,
  idx INTEGER NOT NULL, device TEXT NOT NULL, at INTEGER NOT NULL);";

fn sqlite(path: &Path, schema: &str, rows: &[&str]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut db = quench_sqlite::Connection::open_or_create(path).unwrap();
    db.execute(schema, &[]).unwrap();
    for sql in rows {
        db.execute(sql, &[]).unwrap();
    }
}

/// A 0.1 library: two artists, a song with a link on two artists, a follow, a playlist, three plays.
fn old_files(dir: &Scratch) -> Options {
    let meta = dir.path().join("old").join("meta");
    sqlite(
        &meta.join("library.db"),
        LIBRARY_SCHEMA,
        &[
            "INSERT INTO artists(id, deezer_id, name, listed_as) VALUES (7, 359, 'Nate Dogg', 'Nate Dogg'), (9000000000, 1, 'Snoop Dogg', NULL)",
            "INSERT INTO albums(id, deezer_id, title, year, type, genre, owner_id) VALUES (3, 8000000000, 'Ghetto Preacher', '2001', 'album', 'Rap/Hip Hop', 7)",
            "INSERT INTO songs(id, key, title, filename, duration, yt_url, yt_channel, yt_title, yt_official, yt_score, yt_manual, yt_status, yt_candidate, rank, yt_views) \
             VALUES (11, 'nate dogg||never leave me alone', 'Never Leave Me Alone', 'Nate Dogg, Snoop Dogg - Never Leave Me Alone', 250, 'https://y/1', 'Nate Dogg - Topic', 'Never Leave Me Alone', 1, 8.5, 0, 'ok', '', 700000, 123456789012)",
            "INSERT INTO songs(id, key, title, filename) VALUES (12, 'nate dogg||x', 'X', 'Nate Dogg - X')",
            "INSERT INTO song_artist VALUES (11, 7, 0), (11, 9000000000, 1), (12, 7, 0)",
            "INSERT INTO song_album VALUES (11, 3, 1), (12, 3, NULL)",
            "INSERT INTO song_track VALUES (5000000000, 11)",
            "INSERT INTO tags(id, kind, name) VALUES (4, 'genre', 'Rap/Hip Hop')",
            "INSERT INTO song_tag VALUES (11, 4)",
            "INSERT INTO follows(id, name, pin, category, position, deezer_name, deezer_id, fans, note, genres) VALUES (2, 'Nate Dogg', '', 'West Coast', 1, 'Nate Dogg', '359', '100', '', 'Rap/Hip Hop')",
            "INSERT INTO categories(id, name, position) VALUES (1, 'West Coast', 1)",
            "INSERT INTO blacklist(id, artist, album_key, title) VALUES (6, 'Nate Dogg', 'greatest hits', 'Greatest Hits')",
            "INSERT INTO suggestions(id, artist, deezer_id, fans, related_to_count, related_to, scope, position) VALUES (1, 'Warren G', '55', '10', 2, 'Nate Dogg', '', 0)",
            "INSERT INTO suggestion_tracks VALUES ('55', 0, 'Regulate', 'Regulate...', 9)",
            "INSERT INTO meta VALUES ('key_scheme', 'unicode-1'), ('schedule', '{\"enabled\":false}')",
        ],
    );
    sqlite(
        &meta.join("user.db"),
        USER_SCHEMA,
        &[
            "INSERT INTO playlists(id, owner, name, created) VALUES (4, 'ann', 'Road trip', 1000)",
            "INSERT INTO playlist_songs VALUES (4, 0, 12), (4, 1, 11)",
            "INSERT INTO plays(owner, song_id, at) VALUES ('ann', 11, 100), ('ann', 12, 100), ('ann', 11, 300)",
            "INSERT INTO contexts VALUES ('ann', 'album:3', 'album', 'Ghetto Preacher', 'Nate Dogg', 'album', '3', '{}', 0, 500)",
            "INSERT INTO resume VALUES ('ann', 11, 61000, '12,11', 1, 'phone', 900)",
        ],
    );
    Options {
        library: meta.join("library.db"),
        user: Some(meta.join("user.db")),
        replace: false,
        dry_run: false,
    }
}

fn no_log(_line: String) {}

async fn rows(db: &mut Db, sql: &str) -> Vec<Vec<songbook_service::index::Value>> {
    db.query(sql, &[]).await.unwrap().rows
}

#[quench_test::test]
async fn a_library_moves_with_its_ids_and_the_counters_go_past_them() {
    if !have_database() {
        return;
    }
    let dir = Scratch::new();
    let pool = pool_in(&dir).await;
    let options = old_files(&dir);
    let report = run(&pool, &options, &no_log).await.unwrap();
    let by = |name: &str| report.iter().find(|t| t.name == name).unwrap();
    assert_eq!((by("songs").source, by("songs").target), (Some(2), 2));
    assert_eq!((by("plays").source, by("plays").target), (Some(3), 3));
    assert!(report.iter().all(|t| t.source == Some(t.target)));

    let mut db = Db::checkout(&pool).await.unwrap();
    // Ids, big numbers and NULLs survive.
    let artists = rows(
        &mut db,
        "SELECT id, deezer_id, listed_as FROM artists ORDER BY id",
    )
    .await;
    assert_eq!(artists[0][0].as_integer(), Some(7));
    assert_eq!(artists[1][0].as_integer(), Some(9_000_000_000));
    assert!(artists[1][2].is_null());
    let song = rows(
        &mut db,
        "SELECT yt_url, yt_score, yt_views, duration, rank FROM songs WHERE id = 11",
    )
    .await;
    assert_eq!(str_at(&song[0], 0), "https://y/1");
    assert_eq!(song[0][1].as_real(), Some(8.5));
    assert_eq!(song[0][2].as_integer(), Some(123_456_789_012));
    let no_track = rows(
        &mut db,
        "SELECT track_no FROM song_album WHERE song_id = 12",
    )
    .await;
    assert!(no_track[0][0].is_null());
    assert_eq!(
        str_at(
            &rows(&mut db, "SELECT value FROM meta WHERE key = 'schedule'").await[0],
            0
        ),
        "{\"enabled\":false}"
    );
    // The plays keep the order they were written in.
    let plays = rows(&mut db, "SELECT song_id FROM plays ORDER BY id").await;
    assert_eq!(
        plays
            .iter()
            .map(|r| r[0].as_integer().unwrap())
            .collect::<Vec<_>>(),
        vec![11, 12, 11]
    );
    // The next row the service writes gets an id past every copied one.
    let next_artist = db
        .insert(
            "INSERT INTO artists(deezer_id, name) VALUES (1234, 'New')",
            &[],
        )
        .await
        .unwrap();
    assert_eq!(next_artist, 9_000_000_001);
    let next_playlist = db
        .insert(
            "INSERT INTO playlists(owner, name, created) VALUES ('ann', 'Two', 1)",
            &[],
        )
        .await
        .unwrap();
    assert_eq!(next_playlist, 5);
    let next_follow = db
        .insert("INSERT INTO follows(name) VALUES ('Next')", &[])
        .await
        .unwrap();
    assert_eq!(next_follow, 3);
    let next_play = db
        .insert(
            "INSERT INTO plays(owner, song_id, at) VALUES ('ann', 12, 9)",
            &[],
        )
        .await
        .unwrap();
    assert_eq!(next_play, 4);
    drop(db);
}

#[quench_test::test]
async fn a_target_that_holds_rows_is_refused_unless_it_is_replaced_and_a_dry_run_changes_nothing() {
    if !have_database() {
        return;
    }
    let dir = Scratch::new();
    let pool = pool_in(&dir).await;
    let mut options = old_files(&dir);

    options.dry_run = true;
    let report = run(&pool, &options, &no_log).await.unwrap();
    assert_eq!(
        report.iter().find(|t| t.name == "songs").unwrap().target,
        2,
        "a dry run copies and checks"
    );
    let mut db = Db::checkout(&pool).await.unwrap();
    assert_eq!(
        rows(&mut db, "SELECT COUNT(*) FROM songs").await[0][0].as_integer(),
        Some(0),
        "and rolls back"
    );
    drop(db);

    options.dry_run = false;
    run(&pool, &options, &no_log).await.unwrap();
    let again = run(&pool, &options, &no_log).await.unwrap_err();
    assert!(again.contains("already holds"), "{again}");
    assert!(again.contains("--replace"), "{again}");

    // Something the 0.2 service wrote since is replaced by the file's rows.
    let mut db = Db::checkout(&pool).await.unwrap();
    db.execute("INSERT INTO follows(name) VALUES ('Written since')", &[])
        .await
        .unwrap();
    drop(db);
    options.replace = true;
    run(&pool, &options, &no_log).await.unwrap();
    let mut db = Db::checkout(&pool).await.unwrap();
    let follows = rows(&mut db, "SELECT name FROM follows").await;
    assert_eq!(follows.len(), 1);
    assert_eq!(str_at(&follows[0], 0), "Nate Dogg");
    drop(db);
}

#[quench_test::test]
async fn a_copy_that_fails_half_way_leaves_nothing_behind() {
    if !have_database() {
        return;
    }
    let dir = Scratch::new();
    let pool = pool_in(&dir).await;
    let options = old_files(&dir);
    // SQLite keeps whatever it is given: a play with a time that is not a number. The library tables are copied first and
    // the playlists file fails, so the whole copy has to be undone.
    let mut user = quench_sqlite::Connection::open(options.user.clone().unwrap()).unwrap();
    user.execute("UPDATE plays SET at = 'soon' WHERE song_id = 12", &[])
        .unwrap();
    drop(user);
    let err = run(&pool, &options, &no_log).await.unwrap_err();
    assert!(err.contains("plays"), "{err}");
    let mut db = Db::checkout(&pool).await.unwrap();
    for table in ["songs", "artists", "follows", "plays"] {
        assert_eq!(
            rows(&mut db, &format!("SELECT COUNT(*) FROM {table}")).await[0][0].as_integer(),
            Some(0),
            "{table} is empty again"
        );
    }
    drop(db);

    let mut missing = options;
    missing.library = PathBuf::from(dir.path()).join("nope.db");
    assert!(
        run(&pool, &missing, &no_log)
            .await
            .unwrap_err()
            .contains("does not exist")
    );
}

#[quench_test::test]
async fn an_old_file_without_the_later_columns_and_a_missing_playlists_file_still_move() {
    if !have_database() {
        return;
    }
    let dir = Scratch::new();
    let pool = pool_in(&dir).await;
    let meta = dir.path().join("older").join("meta");
    // The first release: no rank, yt_views or genres.
    let old_schema = LIBRARY_SCHEMA
        .replace(",\n  rank INTEGER, yt_views INTEGER", "")
        .replace(", genres TEXT NOT NULL DEFAULT ''", "");
    assert!(
        !old_schema.contains("yt_views"),
        "the fixture drops the later columns"
    );
    sqlite(
        &meta.join("library.db"),
        &old_schema,
        &[
            "INSERT INTO songs(id, key, title, filename) VALUES (1, 'a||b', 'B', 'A - B')",
            "INSERT INTO follows(id, name) VALUES (1, 'A')",
        ],
    );
    let options = Options {
        library: meta.join("library.db"),
        user: Some(meta.join("user.db")),
        replace: false,
        dry_run: false,
    };
    let lines = Mutex::new(Vec::new());
    let log = |line: String| lines.lock().unwrap().push(line);
    let report = run(&pool, &options, &log).await.unwrap();
    let by = |name: &str| report.iter().find(|t| t.name == name).unwrap();
    assert_eq!(by("songs").target, 1);
    assert_eq!(by("playlists").source, None, "no user.db: nothing to move");
    let said = lines.lock().unwrap().join("\n");
    assert!(said.contains("user.db does not exist"), "{said}");
    let mut db = Db::checkout(&pool).await.unwrap();
    let song = rows(&mut db, "SELECT rank, yt_views, yt_url FROM songs").await;
    assert!(song[0][0].is_null() && song[0][1].is_null());
    assert_eq!(
        str_at(&song[0], 2),
        "",
        "a column the file lacks takes the target's default"
    );
    assert_eq!(
        str_at(&rows(&mut db, "SELECT genres FROM follows").await[0], 0),
        ""
    );
    drop(db);
}

#[quench_test::test]
fn the_arguments_name_the_files_the_database_and_the_mode() {
    let args = |list: &[&str]| list.iter().map(ToString::to_string).collect::<Vec<_>>();
    let (options, url, schema) = parse_args(&args(&[
        "--home",
        "/mnt/dev/music",
        "--database-url",
        "postgres://u@h/db",
        "--schema",
        "sb",
        "--replace",
        "--dry-run",
    ]))
    .unwrap();
    assert_eq!(
        options.library,
        PathBuf::from("/mnt/dev/music/meta/library.db")
    );
    assert_eq!(
        options.user,
        Some(PathBuf::from("/mnt/dev/music/meta/user.db"))
    );
    assert_eq!((url.as_str(), schema.as_str()), ("postgres://u@h/db", "sb"));
    assert!(options.replace && options.dry_run);

    let (options, _, _) = parse_args(&args(&[
        "--database-url",
        "postgres://u@h/db",
        "--library",
        "/x/l.db",
        "--user",
        "/x/u.db",
    ]))
    .unwrap();
    assert_eq!(options.library, PathBuf::from("/x/l.db"));
    assert_eq!(options.user, Some(PathBuf::from("/x/u.db")));
    assert!(!options.replace && !options.dry_run);

    assert!(
        parse_args(&args(&["--database-url"]))
            .unwrap_err()
            .contains("needs a value")
    );
    assert!(
        parse_args(&args(&["--nope", "--database-url", "x"]))
            .unwrap_err()
            .contains("unknown option")
    );
}
