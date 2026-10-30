//! The browser client: the page that starts it and the files it loads (`forge-web` does the serving).
//!
//! These routes are public (`Auth` covers only `/api/v1`); the client signs in at gatehouse itself and then calls the API
//! like any other. The module is read from `WEB_DIR` (see `forge_web`).

use forge_web::Client;
use quench_http::prelude::*;

static CLIENT: Client = Client::new("Daybook", daybook_web::stylesheet)
    .framed()
    .icon(include_bytes!("../../../icon.png"));

#[get("/ui")]
pub async fn page() -> Response {
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
    let _ = page as fn() -> _;
    let _ = script as fn() -> _;
    let _ = style as fn() -> _;
    let _ = shell as fn() -> _;
    let _ = favicon as fn() -> _;
    let _ = module as fn() -> _;
}
