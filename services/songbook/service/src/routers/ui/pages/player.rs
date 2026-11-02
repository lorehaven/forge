//! The player: what everyone signed in sees at `/ui`. One page for every address of its router (the client draws the screen
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

macro_rules! screens {
    ($($name:ident => $path:literal),* $(,)?) => {
        $(
            #[get($path)]
            pub(super) async fn $name(who: ActorOrRedirect) -> Response {
                page(who)
            }
        )*

        fn register_screens() {
            $( let _ = $name as fn(_) -> _; )*
        }
    };
}

screens! {
    home => "/ui",
    home_slash => "/ui/",
    search => "/ui/search",
    artists => "/ui/artists",
    artist => "/ui/artist/{name}",
    album => "/ui/album/{id}",
    category => "/ui/category/{name}",
    playlists => "/ui/playlists",
    playlist => "/ui/playlist/{id}",
    saved => "/ui/saved",
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
    register_screens();
    let _ = script as fn() -> _;
    let _ = style as fn() -> _;
    let _ = engine as fn() -> _;
    let _ = worker as fn() -> _;
    let _ = favicon as fn() -> _;
    let _ = module as fn() -> _;
}
