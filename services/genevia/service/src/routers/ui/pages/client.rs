//! The browser client: the pages that start it (behind the session check) and the files it loads (`forge-web` serves them).
//!
//! The client routes inside `/ui`: the list of trees at `/ui/home`, and `/ui/t/<tree>/<view>/<person>`. `/ui` itself
//! sends the visitor to `/ui/home`, which is where gatehouse sends a browser after signing in.

use crate::routers::ui::common::ActorOrRedirect;
use forge_web::Client;
use quench_http::http::StatusCode;
use quench_http::prelude::*;
use quench_starter::common::routes::with_base_path;

static CLIENT: Client = Client::new("Genevia", genevia_web::stylesheet)
    .framed()
    .icon(include_bytes!("../../../../../icon.png"));

/// The page, or the way to sign in.
fn page(who: ActorOrRedirect) -> Response {
    match who.or_redirect() {
        Ok(_) => CLIENT.page(),
        Err(redirect) => redirect,
    }
}

fn to_home() -> Response {
    Response::new(StatusCode::FOUND).header("Location", with_base_path("/ui/home"))
}

#[get("/ui")]
pub(super) async fn root() -> Response {
    to_home()
}

#[get("/ui/")]
pub(super) async fn root_slash() -> Response {
    to_home()
}

#[get("/ui/home")]
pub(super) async fn home(who: ActorOrRedirect) -> Response {
    page(who)
}

#[get("/ui/home/")]
pub(super) async fn home_slash(who: ActorOrRedirect) -> Response {
    page(who)
}

#[get("/ui/t/{tree}")]
pub(super) async fn tree(who: ActorOrRedirect, Path(_tree): Path<String>) -> Response {
    page(who)
}

#[get("/ui/t/{tree}/{view}")]
pub(super) async fn view(
    who: ActorOrRedirect,
    Path((_tree, _view)): Path<(String, String)>,
) -> Response {
    page(who)
}

#[get("/ui/t/{tree}/{view}/{person}")]
pub(super) async fn person(
    who: ActorOrRedirect,
    Path((_tree, _view, _person)): Path<(String, String, String)>,
) -> Response {
    page(who)
}

// The files carry no data of their own: the trees come from the API, which has its own check.
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
    let _ = root as fn() -> _;
    let _ = root_slash as fn() -> _;
    let _ = home as fn(_) -> _;
    let _ = home_slash as fn(_) -> _;
    let _ = tree as fn(_, _) -> _;
    let _ = view as fn(_, _) -> _;
    let _ = person as fn(_, _) -> _;
    let _ = script as fn() -> _;
    let _ = style as fn() -> _;
    let _ = shell as fn() -> _;
    let _ = favicon as fn() -> _;
    let _ = module as fn() -> _;
}
