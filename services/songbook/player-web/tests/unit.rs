//! The requests as calls, and the answers as messages.
#![allow(clippy::unwrap_used)]

use songbook_core::{AlbumQuery, PlaylistInfo, SongQuery};
use songbook_player::live::Intent;
use songbook_player::{Msg, Request};
use songbook_player_web::protocol::{base_of, call_for, interpret};

#[quench_test::test]
fn the_prefix_is_found_in_the_page_path() {
    assert_eq!(base_of("/songbook/ui/album/7"), "/songbook");
    assert_eq!(base_of("/songbook/ui"), "/songbook");
    assert_eq!(base_of("/ui/search"), "");
}

#[quench_test::test]
fn requests_become_calls() {
    let c = call_for(&Request::Songs(
        SongQuery {
            artist: "Nate Dogg".into(),
            limit: 500,
            ..SongQuery::default()
        },
        "k".into(),
    ));
    assert_eq!(
        (c.method, c.path.as_str()),
        ("GET", "/api/v1/library/songs?artist=Nate%20Dogg&limit=500")
    );
    assert_eq!(call_for(&Request::Home).path, "/api/v1/library/home");
    let c = call_for(&Request::Albums(AlbumQuery::default(), "home".into()));
    assert_eq!(c.path, "/api/v1/library/albums");
    let c = call_for(&Request::Search("a b".into()));
    assert_eq!(c.path, "/api/v1/library/search?q=a%20b");
    let c = call_for(&Request::AddToPlaylist(4, vec![1, 2]));
    assert_eq!(
        (c.method, c.path.as_str(), c.body.as_deref()),
        (
            "POST",
            "/api/v1/playlists/4/songs",
            Some(r#"{"song_ids":[1,2]}"#)
        )
    );
    let c = call_for(&Request::SetPlaylist(4, vec![2]));
    assert_eq!(c.method, "PUT");
    let c = call_for(&Request::DeletePlaylist(4));
    assert_eq!((c.method, c.body), ("DELETE", None));
}

#[quench_test::test]
fn answers_become_messages_for_the_page_that_asked() {
    let ids = Request::Ids(SongQuery::default(), Intent::Play { shuffle: true });
    assert_eq!(
        interpret(&ids, 200, r#"{"ids":[3,1],"total":2}"#),
        Some(Msg::Ids(
            Intent::Play { shuffle: true },
            songbook_core::SongIds {
                ids: vec![3, 1],
                total: 2
            }
        ))
    );
    let made = Request::CreatePlaylist("Road".into());
    assert_eq!(
        interpret(&made, 200, r#"{"id":4,"name":"Road","songs":0}"#),
        Some(Msg::Playlist(PlaylistInfo {
            id: 4,
            name: "Road".into(),
            songs: 0
        }))
    );
    assert_eq!(interpret(&Request::Overview, 401, ""), Some(Msg::SignedOut));
    assert!(
        matches!(interpret(&made, 400, r#"{"error":"A playlist needs a name."}"#), Some(Msg::Rejected(w)) if w == "A playlist needs a name.")
    );
    assert!(matches!(
        interpret(&Request::Overview, 200, "nonsense"),
        Some(Msg::Failed(_))
    ));
    assert!(matches!(
        interpret(&Request::Overview, 502, ""),
        Some(Msg::Failed(_))
    ));
}

quench_test::main!();
