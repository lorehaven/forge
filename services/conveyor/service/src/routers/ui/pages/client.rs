//! The browser client: the page that starts it (for a signed-in visitor) and the files it loads (`forge-web` serves them).

use crate::routers::ui::common::ActorOrRedirect;
use forge_web::Client;
use quench_http::prelude::*;

static CLIENT: Client = Client::new("Conveyor", conveyor_web::stylesheet)
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

#[get("/ui/projects/{id}")]
pub(super) async fn project(who: ActorOrRedirect, Path(_id): Path<String>) -> Response {
    page(who)
}

#[get("/ui/runs")]
pub(super) async fn runs(who: ActorOrRedirect) -> Response {
    page(who)
}

#[get("/ui/runs/{id}")]
pub(super) async fn run(who: ActorOrRedirect, Path(_id): Path<String>) -> Response {
    page(who)
}

#[get("/ui/credentials")]
pub(super) async fn credentials(who: ActorOrRedirect) -> Response {
    page(who)
}

#[get("/ui/repos")]
pub(super) async fn repos(who: ActorOrRedirect) -> Response {
    page(who)
}

#[get("/ui/repos/{owner}/{name}/edit")]
pub(super) async fn repo_edit(
    who: ActorOrRedirect,
    Path((_owner, _name)): Path<(String, String)>,
) -> Response {
    page(who)
}

#[get("/ui/repos/{owner}/{name}/run")]
pub(super) async fn repo_launch(
    who: ActorOrRedirect,
    Path((_owner, _name)): Path<(String, String)>,
) -> Response {
    page(who)
}

#[get("/ui/repos/{owner}/{name}/scan")]
pub(super) async fn repo_scan(
    who: ActorOrRedirect,
    Path((_owner, _name)): Path<(String, String)>,
) -> Response {
    page(who)
}

#[get("/ui/repos/{owner}/{name}/scan/{category}")]
pub(super) async fn repo_check(
    who: ActorOrRedirect,
    Path((_owner, _name, _category)): Path<(String, String, String)>,
) -> Response {
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
    let _ = project as fn(_, _) -> _;
    let _ = runs as fn(_) -> _;
    let _ = run as fn(_, _) -> _;
    let _ = credentials as fn(_) -> _;
    let _ = repos as fn(_) -> _;
    let _ = repo_edit as fn(_, _) -> _;
    let _ = repo_launch as fn(_, _) -> _;
    let _ = repo_scan as fn(_, _) -> _;
    let _ = repo_check as fn(_, _) -> _;
    let _ = script as fn() -> _;
    let _ = style as fn() -> _;
    let _ = shell as fn() -> _;
    let _ = favicon as fn() -> _;
    let _ = module as fn() -> _;
}
