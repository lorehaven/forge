//! The browser client: the page that starts it (for a signed-in visitor) and the files it loads (`forge-web` serves them).

use crate::routers::ui::common::ActorOrRedirect;
use forge_web::Client;
use quench_http::prelude::*;

static CLIENT: Client = Client::new("Gantry", gantry_web::stylesheet)
    .framed()
    .icon(include_bytes!("../../../../../icon.png"));

/// The page, or the way to sign in.
fn page(who: ActorOrRedirect) -> Response {
    match who.or_redirect() {
        Ok(_) => CLIENT.page(),
        Err(redirect) => redirect,
    }
}

#[get("/ui/home")]
pub(super) async fn home(who: ActorOrRedirect) -> Response {
    page(who)
}

#[get("/ui/home/")]
pub(super) async fn home_slash(who: ActorOrRedirect) -> Response {
    page(who)
}

#[get("/ui/apps/{name}")]
pub(super) async fn package(who: ActorOrRedirect, Path(_name): Path<String>) -> Response {
    page(who)
}

#[get("/ui/targets/{name}")]
pub(super) async fn target(who: ActorOrRedirect, Path(_name): Path<String>) -> Response {
    page(who)
}

#[get("/ui/operations")]
pub(super) async fn operations(who: ActorOrRedirect) -> Response {
    page(who)
}

#[get("/ui/operations/{id}")]
pub(super) async fn operation(who: ActorOrRedirect, Path(_id): Path<String>) -> Response {
    page(who)
}

#[get("/ui/plans/{id}")]
pub(super) async fn plan(who: ActorOrRedirect, Path(_id): Path<String>) -> Response {
    page(who)
}

#[get("/ui/resource")]
pub(super) async fn resource(who: ActorOrRedirect) -> Response {
    page(who)
}

// The files carry no data of their own: the data comes from the API, which has its own check.
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
    let _ = home_slash as fn(_) -> _;
    let _ = package as fn(_, _) -> _;
    let _ = target as fn(_, _) -> _;
    let _ = operations as fn(_) -> _;
    let _ = operation as fn(_, _) -> _;
    let _ = plan as fn(_, _) -> _;
    let _ = resource as fn(_) -> _;
    let _ = script as fn() -> _;
    let _ = style as fn() -> _;
    let _ = shell as fn() -> _;
    let _ = favicon as fn() -> _;
    let _ = module as fn() -> _;
}
