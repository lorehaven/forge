//! Serves the app: the page rendered on the server, the module, and nothing else.
//!
//! `PORT` sets the port (3000), `WASM` the module to serve unless it is built in. `quench dev` sets
//! `QUENCH_DEV`, which makes every page reload when the server restarts.

use std::sync::Arc;

use quench_fullstack::{Page, Ui, UiApp};
use quench_http::prelude::*;

const STYLE: &str = "body{font-family:system-ui,sans-serif;margin:2rem auto;max-width:32rem;padding:0 1rem}\
button{margin:.25rem;min-height:2.5rem;min-width:2.5rem}input{min-height:2.5rem}";

#[cfg(feature = "embed")]
fn module() -> Vec<u8> {
    include_bytes!("../../dist/app.wasm").to_vec()
}

#[cfg(not(feature = "embed"))]
fn module() -> Vec<u8> {
    let path = std::env::var("WASM")
        .unwrap_or_else(|_| "target/wasm32-unknown-unknown/wasm-dev/hoist_app.wasm".to_string());
    std::fs::read(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e} - run `quench build`"))
}

#[tokio::main]
async fn main() {
    let port = std::env::var("PORT").unwrap_or_else(|_| "3000".to_string());
    let mut ui = Ui::new(module());
    if std::env::var_os("QUENCH_DEV").is_some() {
        ui = ui.dev_reload();
    }
    let style = ui.add_asset("site.css", STYLE, "text/css");
    let home = move || Page::new("hoist", hoist_app::app()).stylesheet(&style);
    let export = home.clone();
    let app = UiApp::new(ui).page("/", move |_req| home());
    // `quench build --bundle` runs the server with QUENCH_EXPORT=<dir>: the page and the assets are
    // written out as a static bundle (what the Android host serves) and the server does not start.
    if let Some(dir) = std::env::var_os("QUENCH_EXPORT") {
        app.ui()
            .export(std::path::Path::new(&dir), export())
            .expect("cannot write the bundle");
        println!("bundle written to {}", std::path::Path::new(&dir).display());
        return;
    }
    let container = Arc::new(ContainerBuilder::new().build().await.expect("container"));
    let addr = format!("127.0.0.1:{port}").parse().expect("address");
    println!("http://{addr}");
    serve_http(Arc::new(app), container, addr)
        .await
        .expect("server");
}
