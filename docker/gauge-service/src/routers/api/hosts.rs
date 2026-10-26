//! `GET /api/v1/hosts` and `GET /api/v1/hosts/{name}` - what the dashboard shows, as JSON.

use crate::domain::model::View;
use crate::domain::store::{Host, Point, Store};
use crate::routers::api::json_error;
use quench_http::prelude::{Inject, Path, Response, get, http::StatusCode};
use quench_serde::Serialize;

#[derive(Serialize)]
struct HostJson {
    name: String,
    kind: &'static str,
    url: String,
    up: bool,
    error: Option<String>,
    last_ok: Option<f64>,
    view: Option<View>,
}

#[derive(Serialize)]
struct HostDetail {
    #[serde(flatten)]
    host: HostJson,
    history: Vec<Point>,
}

fn json(host: &Host) -> HostJson {
    HostJson {
        name: host.name.clone(),
        kind: host.kind.as_str(),
        url: host.url.clone(),
        up: host.up(),
        error: host.error.clone(),
        last_ok: host.last_ok,
        view: host.view.clone(),
    }
}

fn ok<T: Serialize>(value: &T) -> Response {
    Response::json(StatusCode::OK, value)
        .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
}

#[get("/api/v1/hosts")]
pub async fn list(Inject(store): Inject<Store>) -> Response {
    let hosts: Vec<HostJson> = store.all().iter().map(json).collect();
    ok(&hosts)
}

#[get("/api/v1/hosts/{name}")]
pub async fn detail(Inject(store): Inject<Store>, Path(name): Path<String>) -> Response {
    match store.get(&name) {
        Some(host) => ok(&HostDetail {
            host: json(&host),
            history: host.history.iter().cloned().collect(),
        }),
        None => json_error(StatusCode::NOT_FOUND, "no such host"),
    }
}

pub fn register_routes() {
    let _ = list as fn(_) -> _;
    let _ = detail as fn(_, _) -> _;
}
