use crate::support::*;
use songbook_service::index::songs::upsert_song;
use songbook_service::youtube::{
    age_restricted, best_of, has_word, score_video, songs_to_look_up, throttled,
};

#[quench_test::test]
fn words_match_whole_words_only() {
    assert!(has_word("Song (Live at X)", "live"));
    assert!(has_word("Song - Sped Up", "sped up"));
    assert!(!has_word("Deliver", "live"));
    assert!(has_word("Remix!", "remix"));
    assert!(!has_word("Mixtape", "mix"));
}

#[quench_test::test]
fn the_error_texts_are_told_apart() {
    assert!(age_restricted("ERROR: Sign in to confirm your age"));
    assert!(age_restricted("This video is age-restricted"));
    assert!(age_restricted("age restricted"));
    assert!(!age_restricted("HTTP Error 429: Too Many Requests"));
    assert!(throttled("HTTP Error 429: Too Many Requests"));
    assert!(throttled("something HTTP Error 403: Forbidden"));
    assert!(!throttled("Video unavailable"));
}

#[quench_test::test]
fn the_artists_topic_channel_with_the_right_length_beats_an_unofficial_upload() {
    let topic = parse(
        r#"{"id": "a", "title": "Never Leave Me Alone", "channel": "Nate Dogg - Topic", "duration": 250, "view_count": 1000}"#,
    );
    let (score, official) = score_video(&topic, "Nate Dogg", "Never Leave Me Alone", Some(250));
    assert!(official);
    assert!((score - (5.0 + 2.0 + 3.0)).abs() < 0.01, "{score}");

    let fan = parse(
        r#"{"id": "b", "title": "Never Leave Me Alone (Live) official", "channel": "Some Fan", "duration": 300, "view_count": 500000000}"#,
    );
    let (fan_score, fan_official) =
        score_video(&fan, "Nate Dogg", "Never Leave Me Alone", Some(250));
    assert!(!fan_official);
    assert!(fan_score < 4.0, "{fan_score}");

    let (best, accepted) = best_of(
        &[fan, topic],
        "Nate Dogg",
        "Never Leave Me Alone",
        Some(250),
    )
    .unwrap();
    assert!(accepted);
    assert_eq!(
        (best.url.as_str(), best.channel.as_str(), best.official),
        (
            "https://www.youtube.com/watch?v=a",
            "Nate Dogg - Topic",
            true
        )
    );
    // nothing good enough: the best is returned but not accepted
    let (_, accepted) = best_of(
        &[parse(r#"{"id": "c", "title": "Other", "channel": "x"}"#)],
        "Nate Dogg",
        "Never Leave Me Alone",
        None,
    )
    .unwrap();
    assert!(!accepted);
    assert!(best_of(&[], "a", "b", None).is_none());
}

#[quench_test::test]
fn songs_to_look_up_skip_links_manual_links_and_misses_unless_asked() {
    let dir = Scratch::new();
    let index = index_in(&dir);
    index
        .with(|db| {
            let ids: Vec<i64> = ["One", "Two", "Three", "Four"]
                .iter()
                .enumerate()
                .map(|(i, t)| {
                    upsert_song(
                        db,
                        &[(1, "Nas".to_string())],
                        t,
                        None,
                        i64::try_from(i).unwrap() + 1,
                    )
                    .unwrap()
                })
                .collect();
            let other = upsert_song(db, &[(2, "AZ".to_string())], "Five", None, 9)?;
            db.execute(
                "UPDATE songs SET yt_status = 'ok', yt_url = 'u' WHERE id = ?",
                &[songbook_service::index::int(ids[0])],
            )?;
            db.execute(
                "UPDATE songs SET yt_status = 'miss' WHERE id = ?",
                &[songbook_service::index::int(ids[1])],
            )?;
            db.execute(
                "UPDATE songs SET yt_manual = 1, yt_status = 'ok' WHERE id = ?",
                &[songbook_service::index::int(ids[2])],
            )?;
            let titles = |v: Vec<songbook_service::youtube::Wanted>| {
                v.into_iter().map(|w| w.title).collect::<Vec<_>>()
            };
            assert_eq!(
                titles(songs_to_look_up(db, "", false)?),
                vec!["Four", "Five"]
            );
            assert_eq!(
                titles(songs_to_look_up(db, "", true)?),
                vec!["Two", "Four", "Five"]
            );
            db.execute(
                "UPDATE artists SET listed_as = 'AZ' WHERE deezer_id = 2",
                &[],
            )?;
            assert_eq!(titles(songs_to_look_up(db, "AZ", false)?), vec!["Five"]);
            let _ = other;
            Ok(())
        })
        .unwrap();
}
