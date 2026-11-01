//! How the app's requests become calls to the service, and how the answers become messages.

use quench_serde::{Deserialize, json};
use songbook_app::{Msg, Request};
use songbook_core::api::encode;
use songbook_core::{
    AddArtists, Added, BlacklistBody, Browse, CategoryBody, Job, LinkBody, NewCategory,
    RemoveArtist, Removed, Started, State,
};

/// Where the admin API lives, under the service's own prefix.
pub const API: &str = "/api/v1/admin";

/// One call to the service: the path is under the service's prefix (the browser adds it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    pub method: &'static str,
    pub path: String,
    /// The JSON body of a `POST`.
    pub body: Option<String>,
}

/// The service's path prefix from the page's own path: `/songbook/ui/library` is `/songbook`, `/ui/library` is no prefix.
#[must_use]
pub fn base_of(path: &str) -> String {
    let at = path
        .find("/ui/")
        .or_else(|| path.strip_suffix("/ui").map(str::len));
    at.map_or_else(String::new, |i| path[..i].trim_end_matches('/').to_string())
}

const fn get(path: String) -> Call {
    Call {
        method: "GET",
        path,
        body: None,
    }
}

fn post<T: quench_serde::Serialize>(path: &str, body: &T) -> Call {
    Call {
        method: "POST",
        path: format!("{API}{path}"),
        body: Some(json::to_string(body)),
    }
}

/// What to ask of the service for a request; `None` for one the browser answers by itself.
#[must_use]
pub fn call_for(request: &Request) -> Option<Call> {
    Some(match request {
        Request::Language(_) => return None,
        Request::State => get(format!("{API}/state")),
        Request::Browse(filter) => get(format!("{API}/browse{}", filter.query())),
        Request::Job { from } => get(format!("{API}/job?from={from}")),
        Request::SaveLink { track_id, url } => post(
            "/link",
            &LinkBody {
                track_id: *track_id,
                url: url.clone(),
            },
        ),
        Request::Blacklist { artist, album_id } => post(
            "/albums/blacklist",
            &BlacklistBody {
                artist: artist.clone(),
                album_id: *album_id,
            },
        ),
        Request::AddArtists(items) => post(
            "/artists/add",
            &AddArtists {
                items: items.clone(),
            },
        ),
        Request::RemoveArtist(name) => {
            post("/artists/remove", &RemoveArtist { name: name.clone() })
        }
        Request::SetCategory { name, category } => post(
            "/artists/category",
            &CategoryBody {
                name: name.clone(),
                category: category.clone(),
            },
        ),
        Request::AddCategory(name) => post("/categories/add", &NewCategory { name: name.clone() }),
        Request::StartJob(job) => post("/jobs", job),
    })
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

/// What the app is told of an answer.
#[must_use]
pub fn interpret(request: &Request, status: u16, body: &str) -> Msg {
    match status {
        200..=299 => match request {
            Request::State => read::<State>(body, Msg::State),
            Request::Browse(_) => read::<Browse>(body, Msg::Browse),
            Request::Job { .. } => read::<Job>(body, Msg::Job),
            Request::SaveLink { .. } => Msg::LinkSaved,
            Request::Blacklist { .. } => read::<Removed>(body, |r| Msg::Blacklisted(r.removed)),
            Request::AddArtists(_) => read::<Added>(body, |a| Msg::ArtistsAdded(a.added)),
            Request::RemoveArtist(_) => Msg::ArtistRemoved,
            Request::SetCategory { .. } => Msg::CategorySet,
            Request::AddCategory(_) => read::<Added>(body, |a| Msg::CategoryAdded(a.added)),
            Request::StartJob(_) => read::<Started>(body, |s| Msg::Started(s.started)),
            Request::Language(_) => Msg::Failed("nothing to ask".into()),
        },
        401 => Msg::SignedOut,
        403 => Msg::Failed("this account may not use Songbook: ask for access in gatehouse".into()),
        409 if matches!(request, Request::StartJob(_)) => Msg::Started(false),
        409 => Msg::Rejected(reason(body, "a job is running: try again when it finishes")),
        400 | 404 | 422 => Msg::Rejected(reason(body, "the service refused that")),
        other => Msg::Failed(format!("the service answered {other}")),
    }
}

/// A call that did not get an answer.
#[must_use]
pub fn unreachable(why: &str) -> Msg {
    Msg::Failed(format!("cannot reach the service: {why}"))
}

/// One path segment, percent-encoded (re-exported for the tests and the host).
#[must_use]
pub fn segment(s: &str) -> String {
    encode(s)
}
