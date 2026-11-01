use crate::support::*;
use quench_core::async_trait;
use quench_serde::Value;
use songbook_service::deezer::{Deezer, Transport};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

/// Fails a number of times, then answers; counts the calls.
#[derive(Debug)]
struct Flaky {
    fail: u32,
    calls: AtomicU32,
    answer: Value,
}

#[async_trait]
impl Transport for Flaky {
    async fn get(&self, _path: &str, _params: &[(String, String)]) -> Result<Value, String> {
        let n = self.calls.fetch_add(1, Ordering::SeqCst);
        if n < self.fail {
            Err("connection reset".into())
        } else {
            Ok(self.answer.clone())
        }
    }
}

#[quench_test::test]
async fn a_failing_request_is_retried_and_a_quota_answer_waited_out() {
    let dir = Scratch::new();
    let flaky = Arc::new(Flaky {
        fail: 2,
        calls: AtomicU32::new(0),
        answer: parse(r#"{"id": 1}"#),
    });
    let deezer = Deezer::new(flaky.clone(), dir.path().join("cache")).instant();
    assert!(deezer.get("artist/1", &[]).await.is_ok());
    assert_eq!(flaky.calls.load(Ordering::SeqCst), 3);

    let dead = Arc::new(Flaky {
        fail: 100,
        calls: AtomicU32::new(0),
        answer: Value::Null,
    });
    let deezer = Deezer::new(dead.clone(), dir.path().join("cache")).instant();
    assert!(
        deezer
            .get("artist/1", &[])
            .await
            .unwrap_err()
            .contains("artist/1")
    );
    assert_eq!(dead.calls.load(Ordering::SeqCst), 6);

    // an API error other than the quota one means "nothing found"
    let nothing = Arc::new(Flaky {
        fail: 0,
        calls: AtomicU32::new(0),
        answer: parse(r#"{"error": {"code": 800}}"#),
    });
    let deezer = Deezer::new(nothing, dir.path().join("cache")).instant();
    assert!(
        deezer
            .get("artist/1", &[])
            .await
            .unwrap()
            .as_object()
            .unwrap()
            .is_empty()
    );
}

#[quench_test::test]
async fn an_artist_is_found_by_exact_name_with_the_most_fans_or_by_pin() {
    let dir = Scratch::new();
    let app = app_with(
        &dir,
        &[
            (
                "search/artist",
                r#"{"data": [{"id": 5, "name": "Nas Jr", "nb_fan": 999}, {"id": 1, "name": "nas", "nb_fan": 10}, {"id": 2, "name": "Nas", "nb_fan": 50}]}"#,
            ),
            ("artist/77", r#"{"id": 77, "name": "Pinned", "nb_fan": 3}"#),
        ],
    );
    let hit = app.deezer.resolve("Nas", "").await.unwrap().unwrap();
    assert_eq!((hit.id, hit.fans), (2, 50));
    assert!(app.deezer.resolve("Nobody", "").await.unwrap().is_none());
    assert_eq!(
        app.deezer
            .resolve("whatever", "77")
            .await
            .unwrap()
            .unwrap()
            .name,
        "Pinned"
    );
    assert!(
        app.deezer
            .resolve("whatever", "78")
            .await
            .unwrap()
            .is_none()
    );
}

#[quench_test::test]
async fn lists_are_read_page_by_page() {
    let dir = Scratch::new();
    let app = app_with(
        &dir,
        &[
            (
                "artist/1/albums?index=0",
                r#"{"data": [{"id": 10, "title": "B", "record_type": "album", "release_date": "2002-01-01"}], "next": "x"}"#,
            ),
            (
                "artist/1/albums?index=100",
                r#"{"data": [{"id": 11, "title": "A", "record_type": "ep", "release_date": "2001-01-01", "explicit_lyrics": true}]}"#,
            ),
        ],
    );
    let albums = app.deezer.artist_albums(1).await.unwrap();
    assert_eq!(
        albums.iter().map(|a| a.id).collect::<Vec<_>>(),
        vec![11, 10],
        "oldest first"
    );
}

#[quench_test::test]
async fn an_album_and_a_track_are_read_once_and_then_from_the_cache() {
    let dir = Scratch::new();
    let answers = [
        (
            "album/9",
            r#"{"id": 9, "title": "Album", "release_date": "1999-12-31", "record_type": "album", "genres": {"data": [{"name": "Rap"}, {"name": "R&B"}]}}"#,
        ),
        (
            "album/9/tracks",
            r#"{"data": [{"id": 90, "title": "One", "duration": 120, "artist": {"id": 1, "name": "Main"}}]}"#,
        ),
        (
            "track/90",
            r#"{"id": 90, "title": "One (feat. Guest)", "duration": 121, "isrc": "X", "contributors": [{"id": 1, "name": "Main", "role": "Main"}, {"id": 2, "name": "Guest"}]}"#,
        ),
    ];
    let app = app_with(&dir, &answers);
    let album = app.deezer.album(9).await.unwrap();
    assert_eq!(
        (
            album.info.title.as_str(),
            album.info.genres.len(),
            album.tracks.len()
        ),
        ("Album", 2, 1)
    );
    assert_eq!(album.tracks[0].artist, Some((1, "Main".to_string())));
    let track = app.deezer.track(90).await.unwrap().unwrap();
    assert_eq!(
        track.contributors,
        vec![(1, "Main".to_string()), (2, "Guest".to_string())]
    );
    assert!(
        dir.path()
            .join(".deezer_cache")
            .join("album_9.json")
            .is_file()
    );
    assert!(
        dir.path()
            .join(".deezer_cache")
            .join("track_90.json")
            .is_file()
    );

    // a second client with no answers at all reads both from disk
    let offline = Deezer::new(
        Arc::new(songbook_service::deezer::Canned::default()),
        dir.path().join(".deezer_cache"),
    )
    .instant();
    assert_eq!(offline.album(9).await.unwrap().tracks.len(), 1);
    assert_eq!(
        offline.track(90).await.unwrap().unwrap().title,
        "One (feat. Guest)"
    );
    // an unknown track is None, and not cached
    assert!(offline.track(91).await.unwrap().is_none());
}
