//! The admin client: the page that starts it (for a signed-in admin) and the files it loads (`forge-web` serves them).
//! It lives under `/ui/admin`; the player, for everyone else, is at `/ui/home`.

use crate::routers::ui::common::AdminOrRedirect;
use forge_web::Client;
use quench_http::prelude::*;
use quench_starter::common::routes::with_base_path;

static CLIENT: Client = Client::new("Songbook admin", songbook_web::stylesheet)
    .framed()
    .at("/ui/admin")
    .icon(include_bytes!("../../../../../icon.png"));

/// The page, or the way to sign in (or out of here).
fn page(who: AdminOrRedirect) -> Response {
    match who.or_redirect() {
        Ok(_) => CLIENT.page(),
        Err(redirect) => redirect,
    }
}

#[get("/ui/admin")]
pub(super) async fn admin(who: AdminOrRedirect) -> Response {
    admin_redirect(who)
}

#[get("/ui/admin/")]
pub(super) async fn admin_slash(who: AdminOrRedirect) -> Response {
    admin_redirect(who)
}

fn admin_redirect(who: AdminOrRedirect) -> Response {
    match who.or_redirect() {
        Ok(_) => Response::new(quench_http::http::StatusCode::FOUND)
            .header("Location", with_base_path("/ui/admin/library")),
        Err(redirect) => redirect,
    }
}

#[get("/ui/admin/library")]
pub(super) async fn library(who: AdminOrRedirect) -> Response {
    page(who)
}

#[get("/ui/admin/pipeline")]
pub(super) async fn pipeline(who: AdminOrRedirect) -> Response {
    page(who)
}

#[get("/ui/admin/discover")]
pub(super) async fn discover(who: AdminOrRedirect) -> Response {
    page(who)
}

#[get("/ui/admin/blacklist")]
pub(super) async fn blacklist(who: AdminOrRedirect) -> Response {
    page(who)
}

#[get("/ui/admin/schedule")]
pub(super) async fn schedule(who: AdminOrRedirect) -> Response {
    page(who)
}

#[get("/ui/admin/activity")]
pub(super) async fn activity(who: AdminOrRedirect) -> Response {
    page(who)
}

// The files carry no data of their own: the numbers come from the API, which has its own check.
#[get("/ui/admin/assets/quench.js")]
pub(super) async fn script() -> Response {
    CLIENT.script()
}

#[get("/ui/admin/assets/app.css")]
pub(super) async fn style() -> Response {
    CLIENT.style()
}

#[get("/ui/admin/assets/shell.js")]
pub(super) async fn shell() -> Response {
    CLIENT.shell()
}

#[get("/ui/admin/assets/favicon.png")]
pub(super) async fn favicon() -> Response {
    CLIENT.favicon()
}

#[get("/ui/admin/assets/app.wasm")]
pub(super) async fn module() -> Response {
    CLIENT.module()
}

pub(super) fn register_routes() {
    let _ = admin as fn(_) -> _;
    let _ = admin_slash as fn(_) -> _;
    let _ = library as fn(_) -> _;
    let _ = pipeline as fn(_) -> _;
    let _ = discover as fn(_) -> _;
    let _ = schedule as fn(_) -> _;
    let _ = blacklist as fn(_) -> _;
    let _ = activity as fn(_) -> _;
    let _ = script as fn() -> _;
    let _ = style as fn() -> _;
    let _ = shell as fn() -> _;
    let _ = favicon as fn() -> _;
    let _ = module as fn() -> _;
}
