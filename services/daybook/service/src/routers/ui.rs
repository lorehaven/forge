//! The browser client: the page that starts it and the files it loads (`forge-web` does the serving).
//!
//! These routes are public (`Auth` covers only `/api/v1`); the client signs in at gatehouse itself and then calls the API
//! like any other. The module is read from `WEB_DIR` (see `forge_web`).

use forge_web::Client;
use quench_http::http::StatusCode;
use quench_http::prelude::*;
use quench_starter::common::routes::with_base_path;

static CLIENT: Client = Client::new("Daybook", daybook_web::stylesheet)
    .framed()
    .icon(include_bytes!("../../../icon.png"));

fn to_home() -> Response {
    Response::new(StatusCode::FOUND).header("Location", with_base_path("/ui/home"))
}

#[get("/ui")]
pub async fn root() -> Response {
    to_home()
}

#[get("/ui/")]
pub async fn root_slash() -> Response {
    to_home()
}

#[get("/ui/home")]
pub async fn home() -> Response {
    CLIENT.page()
}

#[get("/ui/home/")]
pub async fn home_slash() -> Response {
    CLIENT.page()
}

#[get("/ui/assets/quench.js")]
pub async fn script() -> Response {
    CLIENT.script()
}

#[get("/ui/assets/app.css")]
pub async fn style() -> Response {
    CLIENT.style()
}

#[get("/ui/assets/shell.js")]
pub async fn shell() -> Response {
    CLIENT.shell()
}

#[get("/ui/assets/favicon.png")]
pub async fn favicon() -> Response {
    CLIENT.favicon()
}

#[get("/ui/assets/app.wasm")]
pub async fn module() -> Response {
    CLIENT.module()
}

pub fn register_routes() {
    let _ = root as fn() -> _;
    let _ = root_slash as fn() -> _;
    let _ = home as fn() -> _;
    let _ = home_slash as fn() -> _;
    let _ = script as fn() -> _;
    let _ = style as fn() -> _;
    let _ = shell as fn() -> _;
    let _ = favicon as fn() -> _;
    let _ = module as fn() -> _;
}
