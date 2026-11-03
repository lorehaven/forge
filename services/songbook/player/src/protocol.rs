//! How the player's requests become calls to the service, and how the answers become messages.

use crate::{Msg, Request};
use quench_serde::{Deserialize, json};
use songbook_core::api::encode;
use songbook_core::{
    AlbumInfo, Home, Overview, PlaylistBody, PlaylistInfo, RecentItem, Search, SongIds,
    SongIdsBody, SongList,
};

/// The listener API, under the service's own prefix.
pub const API: &str = "/api/v1";

/// One call to the service: the path is under the service's prefix (the browser adds it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    pub method: &'static str,
    pub path: String,
    /// The JSON body.
    pub body: Option<String>,
}

/// The service's path prefix from the page's own path: `/songbook/ui/album/7` is `/songbook`, `/ui` is no prefix.
#[must_use]
pub fn base_of(path: &str) -> String {
    let at = path
        .find("/ui/")
        .or_else(|| path.strip_suffix("/ui").map(str::len));
    at.map_or_else(String::new, |i| path[..i].trim_end_matches('/').to_string())
}

const fn call(method: &'static str, path: String, body: Option<String>) -> Call {
    Call { method, path, body }
}

const fn get(path: String) -> Call {
    call("GET", path, None)
}

fn send<T: quench_serde::Serialize>(method: &'static str, path: String, body: &T) -> Call {
    call(method, path, Some(json::to_string(body)))
}

/// What to ask of the service for a request.
#[must_use]
pub fn call_for(request: &Request) -> Call {
    match request {
        Request::Overview => get(format!("{API}/library/overview")),
        Request::Home => get(format!("{API}/library/home")),
        Request::Albums(q, _) => get(format!("{API}/library/albums{}", q.query())),
        Request::Songs(q, _) => get(format!("{API}/library/songs{}", q.query())),
        Request::Ids(q, _) => get(format!("{API}/library/ids{}", q.query())),
        Request::Search(text) => get(format!("{API}/library/search?q={}", encode(text))),
        Request::Playlists => get(format!("{API}/playlists")),
        Request::Contexts => get(format!("{API}/history/contexts?limit=20")),
        Request::Remember(item) => send("POST", format!("{API}/history/context"), item),
        Request::CreatePlaylist(name) => send(
            "POST",
            format!("{API}/playlists"),
            &PlaylistBody { name: name.clone() },
        ),
        Request::RenamePlaylist(id, name) => send(
            "PUT",
            format!("{API}/playlists/{id}"),
            &PlaylistBody { name: name.clone() },
        ),
        Request::DeletePlaylist(id) => call("DELETE", format!("{API}/playlists/{id}"), None),
        Request::AddToPlaylist(id, songs) => send(
            "POST",
            format!("{API}/playlists/{id}/songs"),
            &SongIdsBody {
                song_ids: songs.clone(),
            },
        ),
        Request::SetPlaylist(id, songs) => send(
            "PUT",
            format!("{API}/playlists/{id}/songs"),
            &SongIdsBody {
                song_ids: songs.clone(),
            },
        ),
    }
}

#[derive(Debug, Deserialize)]
struct ApiError {
    error: String,
}

fn reason(body: &str, fallback: &str) -> String {
    json::from_str::<ApiError>(body).map_or_else(|_| fallback.to_string(), |e| e.error)
}

fn read<T: Deserialize>(body: &str, into: impl FnOnce(T) -> Msg) -> Msg {
    json::from_str::<T>(body).map_or_else(
        |e| Msg::Failed(format!("the service answered something unreadable: {e}")),
        into,
    )
}

/// What the player is told of an answer. `None` for an answer that needs no telling (a change that was made).
#[must_use]
pub fn interpret(request: &Request, status: u16, body: &str) -> Option<Msg> {
    match status {
        200..=299 => match request {
            Request::Overview => Some(read::<Overview>(body, Msg::Overview)),
            Request::Home => Some(read::<Home>(body, Msg::Home)),
            Request::Albums(_, key) => Some(read::<Vec<AlbumInfo>>(body, |a| {
                Msg::Albums(key.clone(), a)
            })),
            Request::Songs(_, key) => Some(read::<SongList>(body, |l| Msg::Songs(key.clone(), l))),
            Request::Ids(_, intent) => Some(read::<SongIds>(body, |i| Msg::Ids(intent.clone(), i))),
            Request::Search(text) => Some(read::<Search>(body, |s| Msg::Found(text.clone(), s))),
            Request::Playlists => Some(read::<Vec<PlaylistInfo>>(body, Msg::Playlists)),
            Request::Contexts => Some(read::<Vec<RecentItem>>(body, Msg::Contexts)),
            Request::Remember(_) => None,
            Request::CreatePlaylist(_)
            | Request::RenamePlaylist(..)
            | Request::AddToPlaylist(..)
            | Request::SetPlaylist(..) => Some(read::<PlaylistInfo>(body, Msg::Playlist)),
            Request::DeletePlaylist(id) => Some(Msg::PlaylistGone(*id)),
        },
        401 => Some(Msg::SignedOut),
        403 => Some(Msg::Failed(
            "this account may not use Songbook: ask for access in gatehouse".into(),
        )),
        400 | 404 | 422 => Some(Msg::Rejected(reason(body, "the service refused that"))),
        other => Some(Msg::Failed(format!("the service answered {other}"))),
    }
}

/// A call that did not get an answer.
#[must_use]
pub fn unreachable(why: &str) -> Msg {
    Msg::Failed(format!("cannot reach the service: {why}"))
}
