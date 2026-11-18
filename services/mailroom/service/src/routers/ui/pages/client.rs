//! The browser client: the page that starts it (for a signed-in visitor) and the files it loads (`forge-web` serves them).

use crate::routers::ui::common::ActorOrRedirect;
use forge_web::Client;
use quench_http::prelude::*;

static CLIENT: Client = Client::new("Mailroom", mailroom_web::stylesheet)
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

#[get("/ui/mail/{account}/{folder}")]
pub(super) async fn folder(
    who: ActorOrRedirect,
    Path((_account, _folder)): Path<(String, String)>,
) -> Response {
    page(who)
}

#[get("/ui/message/{id}")]
pub(super) async fn message(who: ActorOrRedirect, Path(_id): Path<String>) -> Response {
    page(who)
}

#[get("/ui/compose")]
pub(super) async fn compose(who: ActorOrRedirect) -> Response {
    page(who)
}

#[get("/ui/accounts")]
pub(super) async fn accounts(who: ActorOrRedirect) -> Response {
    page(who)
}

#[get("/ui/admin")]
pub(super) async fn admin(who: ActorOrRedirect) -> Response {
    page(who)
}

#[get("/ui/admin/{section}")]
pub(super) async fn admin_section(who: ActorOrRedirect, Path(_section): Path<String>) -> Response {
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
    let _ = folder as fn(_, _) -> _;
    let _ = message as fn(_, _) -> _;
    let _ = compose as fn(_) -> _;
    let _ = accounts as fn(_) -> _;
    let _ = admin as fn(_) -> _;
    let _ = admin_section as fn(_, _) -> _;
    let _ = script as fn() -> _;
    let _ = style as fn() -> _;
    let _ = shell as fn() -> _;
    let _ = favicon as fn() -> _;
    let _ = module as fn() -> _;
}
