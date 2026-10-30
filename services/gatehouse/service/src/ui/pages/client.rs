//! The browser client: the page that starts it and the files it loads (`forge-web` serves them).
//!
//! Gatehouse is the one service whose pages open without a session (sign-in, registration, password reset, the links in
//! emails), so there are two doors: [`public`] serves the page as it is, [`signed_in`] is for a handler that has already
//! checked the session. The client draws the screen from its path; what is behind a session comes from the page API.

use forge_web::Client;
use quench_http::prelude::*;

static CLIENT: Client = Client::new("Gatehouse", gatehouse_web::stylesheet)
    .framed()
    .icon(include_bytes!("../../../../icon.png"));

/// The page, for a screen that needs no session.
#[must_use]
pub fn public() -> Response {
    CLIENT.page()
}

/// The page, for a screen behind a session the handler has checked.
#[must_use]
pub fn signed_in() -> Response {
    CLIENT.page()
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

/// The icon of a service, for its card on the dashboard: the same picture the service shows in its own tab.
#[get("/ui/assets/icons/{name}")]
pub(super) async fn service_icon(Path(name): Path<String>) -> Response {
    let png: &'static [u8] = match name.trim_end_matches(".png") {
        "conveyor" => include_bytes!("../../../../../conveyor/icon.png"),
        "daybook" => include_bytes!("../../../../../daybook/icon.png"),
        "gantry" => include_bytes!("../../../../../gantry/icon.png"),
        // The realm's own card, the user list.
        "id-card" => include_bytes!("../../../../id-card.png"),
        "gauge" => include_bytes!("../../../../../gauge/icon.png"),
        "hoist" => include_bytes!("../../../../../hoist/icon.png"),
        "sage" => include_bytes!("../../../../../sage/icon.png"),
        "switchboard" => include_bytes!("../../../../../switchboard/icon.png"),
        "warehouse" => include_bytes!("../../../../../warehouse/icon.png"),
        "workbench" => include_bytes!("../../../../../workbench/icon.png"),
        _ => return Response::not_found(),
    };
    Response::from_bytes(quench_http::prelude::http::StatusCode::OK, png.into())
        .header("content-type", "image/png")
        .header("cache-control", "public, max-age=3600")
}

#[get("/ui/assets/app.wasm")]
pub(super) async fn module() -> Response {
    CLIENT.module()
}

pub(super) fn register_routes() {
    let _ = script as fn() -> _;
    let _ = style as fn() -> _;
    let _ = shell as fn() -> _;
    let _ = favicon as fn() -> _;
    let _ = service_icon as fn(_) -> _;
    let _ = module as fn() -> _;
}
