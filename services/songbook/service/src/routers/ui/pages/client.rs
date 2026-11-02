//! The browser client: the page that starts it (for a signed-in visitor) and the files it loads (`forge-web` serves them).

use crate::routers::ui::common::ActorOrRedirect;
use forge_web::Client;
use quench_http::prelude::*;
use quench_starter::common::routes::with_base_path;

static CLIENT: Client = Client::new("Songbook", songbook_web::stylesheet)
    .framed()
    .icon(include_bytes!("../../../../../icon.png"));

/// The page, or the way to sign in.
fn page(who: ActorOrRedirect) -> Response {
    match who.or_redirect() {
        Ok(_) => CLIENT.page(),
        Err(redirect) => redirect,
    }
}

/// Where a visitor lands after signing in, and from `/ui`.
#[get("/ui/home")]
pub(super) async fn home(who: ActorOrRedirect) -> Response {
    match who.or_redirect() {
        Ok(_) => Response::new(quench_http::http::StatusCode::FOUND)
            .header("Location", with_base_path("/ui/library")),
        Err(redirect) => redirect,
    }
}

#[get("/ui/library")]
pub(super) async fn library(who: ActorOrRedirect) -> Response {
    page(who)
}

#[get("/ui/pipeline")]
pub(super) async fn pipeline(who: ActorOrRedirect) -> Response {
    page(who)
}

#[get("/ui/discover")]
pub(super) async fn discover(who: ActorOrRedirect) -> Response {
    page(who)
}

#[get("/ui/schedule")]
pub(super) async fn schedule(who: ActorOrRedirect) -> Response {
    page(who)
}

#[get("/ui/activity")]
pub(super) async fn activity(who: ActorOrRedirect) -> Response {
    page(who)
}

// The files carry no data of their own: the numbers come from the API, which has its own check.
#[get("/ui/assets/quench.js")]
pub(super) async fn script() -> Response {
    CLIENT.script()
}

#[get("/ui/assets/app.css")]
pub(super) async fn style() -> Response {
    CLIENT.style()
}

#[get("/ui/assets/shell.js")]
pub(super) async fn shell() -> Response {
    CLIENT.shell()
}

#[get("/ui/assets/favicon.png")]
pub(super) async fn favicon() -> Response {
    CLIENT.favicon()
}

#[get("/ui/assets/app.wasm")]
pub(super) async fn module() -> Response {
    CLIENT.module()
}

pub(super) fn register_routes() {
    let _ = home as fn(_) -> _;
    let _ = library as fn(_) -> _;
    let _ = pipeline as fn(_) -> _;
    let _ = discover as fn(_) -> _;
    let _ = schedule as fn(_) -> _;
    let _ = activity as fn(_) -> _;
    let _ = script as fn() -> _;
    let _ = style as fn() -> _;
    let _ = shell as fn() -> _;
    let _ = favicon as fn() -> _;
    let _ = module as fn() -> _;
}
