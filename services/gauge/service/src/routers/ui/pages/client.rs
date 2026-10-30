//! The browser client: the page that starts it (for a signed-in visitor) and the files it loads (`forge-web` serves them).

use crate::routers::ui::common::ActorOrRedirect;
use forge_web::Client;
use quench_http::prelude::*;

static CLIENT: Client = Client::new("Gauge", gauge_web::stylesheet)
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

#[get("/ui/hosts/{name}")]
pub(super) async fn host(who: ActorOrRedirect, Path(_name): Path<String>) -> Response {
    page(who)
}

#[get("/ui/fleet")]
pub(super) async fn fleet(who: ActorOrRedirect) -> Response {
    page(who)
}

#[get("/ui/storage")]
pub(super) async fn storage(who: ActorOrRedirect) -> Response {
    page(who)
}

#[get("/ui/gpus")]
pub(super) async fn gpus(who: ActorOrRedirect) -> Response {
    page(who)
}

#[get("/ui/network")]
pub(super) async fn network(who: ActorOrRedirect) -> Response {
    page(who)
}

#[get("/ui/power")]
pub(super) async fn power(who: ActorOrRedirect) -> Response {
    page(who)
}

#[get("/ui/alerts")]
pub(super) async fn alerts(who: ActorOrRedirect) -> Response {
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
    let _ = home_slash as fn(_) -> _;
    let _ = host as fn(_, _) -> _;
    let _ = fleet as fn(_) -> _;
    let _ = storage as fn(_) -> _;
    let _ = gpus as fn(_) -> _;
    let _ = network as fn(_) -> _;
    let _ = power as fn(_) -> _;
    let _ = alerts as fn(_) -> _;
    let _ = script as fn() -> _;
    let _ = style as fn() -> _;
    let _ = shell as fn() -> _;
    let _ = favicon as fn() -> _;
    let _ = module as fn() -> _;
}
