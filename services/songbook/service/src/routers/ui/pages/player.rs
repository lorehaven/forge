//! The player: what everyone signed in sees at `/ui/home`. One page for every address of its router (the client draws the screen
//! from the address), and the files it loads.

use crate::routers::ui::common::ActorOrRedirect;
use forge_web::Client;
use quench_http::prelude::http::StatusCode;
use quench_http::prelude::*;
use quench_starter::common::routes::with_base_path;

static CLIENT: Client = Client::new("Songbook", songbook_player_web::stylesheet)
    .module_file("player.wasm")
    .with_script("engine.js", songbook_player::ENGINE)
    .media()
    .icon(include_bytes!("../../../../../icon.png"));

/// The page, or the way to sign in.
fn page(who: ActorOrRedirect) -> Response {
    match who.or_redirect() {
        Ok(_) => CLIENT.page(),
        Err(redirect) => redirect,
    }
}

// One handler per screen, written out: the route census and the security scanner read the attributes, and cannot see
// through a macro.
#[get("/ui")]
pub(super) async fn root() -> Response {
    Response::new(StatusCode::FOUND).header("Location", with_base_path("/ui/home"))
}

#[get("/ui/")]
pub(super) async fn root_slash() -> Response {
    Response::new(StatusCode::FOUND).header("Location", with_base_path("/ui/home"))
}

#[get("/ui/home")]
pub(super) async fn home(who: ActorOrRedirect) -> Response {
    page(who)
}

/// The address the home page had before the player was rewritten: portal links and bookmarks still point at it.
#[get("/ui/library")]
pub(super) async fn library_legacy() -> Response {
    Response::new(StatusCode::FOUND).header("Location", with_base_path("/ui/home"))
}

#[get("/ui/search")]
pub(super) async fn search(who: ActorOrRedirect) -> Response {
    page(who)
}

#[get("/ui/artists")]
pub(super) async fn artists(who: ActorOrRedirect) -> Response {
    page(who)
}

#[get("/ui/artist/{name}")]
pub(super) async fn artist(who: ActorOrRedirect) -> Response {
    page(who)
}

#[get("/ui/album/{id}")]
pub(super) async fn album(who: ActorOrRedirect) -> Response {
    page(who)
}

#[get("/ui/category/{name}")]
pub(super) async fn category(who: ActorOrRedirect) -> Response {
    page(who)
}

#[get("/ui/playlists")]
pub(super) async fn playlists(who: ActorOrRedirect) -> Response {
    page(who)
}

#[get("/ui/playlist/{id}")]
pub(super) async fn playlist(who: ActorOrRedirect) -> Response {
    page(who)
}

#[get("/ui/saved")]
pub(super) async fn saved(who: ActorOrRedirect) -> Response {
    page(who)
}

// The files carry no data of their own: the library comes from the API, which has its own check.
#[get("/ui/assets/quench.js")]
pub(super) async fn script() -> Response {
    CLIENT.script()
}

#[get("/ui/assets/app.css")]
pub(super) async fn style() -> Response {
    CLIENT.style()
}

#[get("/ui/assets/engine.js")]
pub(super) async fn engine() -> Response {
    CLIENT.own_script()
}

/// The service worker, at the service's root so that its scope covers the pages and the API (the pictures). It carries no
/// data, so it needs no sign-in.
#[get("/sw.js")]
pub(super) async fn worker() -> Response {
    Response::from_bytes(
        StatusCode::OK,
        songbook_player::SERVICE_WORKER.as_bytes().to_vec().into(),
    )
    .header("content-type", "text/javascript; charset=utf-8")
    .header("cache-control", "no-cache")
    .header("service-worker-allowed", with_base_path("/"))
}

#[get("/ui/assets/favicon.png")]
pub(super) async fn favicon() -> Response {
    CLIENT.favicon()
}

#[get("/ui/assets/player.wasm")]
pub(super) async fn module() -> Response {
    CLIENT.module()
}

pub(super) fn register_routes() {
    let _ = home as fn(_) -> _;
    let _ = root as fn() -> _;
    let _ = root_slash as fn() -> _;
    let _ = search as fn(_) -> _;
    let _ = artists as fn(_) -> _;
    let _ = artist as fn(_) -> _;
    let _ = album as fn(_) -> _;
    let _ = category as fn(_) -> _;
    let _ = playlists as fn(_) -> _;
    let _ = playlist as fn(_) -> _;
    let _ = saved as fn(_) -> _;
    let _ = library_legacy as fn() -> _;
    let _ = script as fn() -> _;
    let _ = style as fn() -> _;
    let _ = engine as fn() -> _;
    let _ = worker as fn() -> _;
    let _ = favicon as fn() -> _;
    let _ = module as fn() -> _;
}
