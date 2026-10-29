//! The browser client: the page that starts it and the files it loads.
//!
//! These routes are public (`Auth` covers only `/api/v1`); the client signs in at gatehouse itself and then calls the API
//! like any other.
//!
//! The wasm module is built apart (`cargo build --target wasm32-unknown-unknown --profile wasm -p daybook-web`) and put
//! in `WEB_DIR` (`web` by default) as `app.wasm`; without it the page says the client is not installed.

use std::path::PathBuf;

use quench_fullstack::{Page, content_security_policy, new_nonce, render_document};
use quench_http::prelude::http::StatusCode;
use quench_http::prelude::*;
use quench_starter::common::routes::with_base_path;
use quench_ui::View;

fn web_dir() -> PathBuf {
    PathBuf::from(quench_core::env::get_or("WEB_DIR", "web"))
}

fn asset(body: impl Into<Vec<u8>>, content_type: &str) -> Response {
    Response::from_bytes(StatusCode::OK, body.into().into())
        .header("content-type", content_type)
        .header("cache-control", "no-cache")
}

/// The page: an empty root, the stylesheet, and the script that loads the module.
#[get("/ui")]
pub async fn page() -> Response {
    if !web_dir().join("app.wasm").exists() {
        return Response::text(
            StatusCode::NOT_FOUND,
            "the browser client is not installed on this server",
        );
    }
    let nonce = new_nonce();
    let html = render_document(
        Page::new("Daybook", View::Empty).stylesheet(&with_base_path("/ui/assets/app.css")),
        &nonce,
        &with_base_path("/ui/assets/quench.js"),
        &with_base_path("/ui/assets/app.wasm"),
    );
    Response::html(StatusCode::OK, html)
        .header("cache-control", "no-store")
        .header("content-security-policy", content_security_policy(&nonce))
        .header("referrer-policy", "same-origin")
}

#[get("/ui/assets/quench.js")]
pub async fn script() -> Response {
    asset(quench_ui_web::QUENCH_JS, "text/javascript; charset=utf-8")
}

#[get("/ui/assets/app.css")]
pub async fn style() -> Response {
    asset(daybook_web::stylesheet(), "text/css; charset=utf-8")
}

#[get("/ui/assets/app.wasm")]
pub async fn module() -> Response {
    std::fs::read(web_dir().join("app.wasm")).map_or_else(
        |_| Response::not_found(),
        |bytes| asset(bytes, "application/wasm"),
    )
}

pub fn register_routes() {
    let _ = page as fn() -> _;
    let _ = script as fn() -> _;
    let _ = style as fn() -> _;
    let _ = module as fn() -> _;
}
