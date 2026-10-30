//! The browser client: the page that starts it (for a signed-in visitor) and the files it loads (`forge-web` serves them).

use crate::routers::ui::common::ActorOrRedirect;
use forge_web::Client;
use quench_http::prelude::*;

static CLIENT: Client = Client::new("Sage", sage_web::stylesheet)
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

#[get("/ui/initializing")]
pub(super) async fn initializing(who: ActorOrRedirect) -> Response {
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
    let _ = initializing as fn(_) -> _;
    let _ = script as fn() -> _;
    let _ = style as fn() -> _;
    let _ = shell as fn() -> _;
    let _ = favicon as fn() -> _;
    let _ = module as fn() -> _;
}
