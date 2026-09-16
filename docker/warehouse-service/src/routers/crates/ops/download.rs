use crate::routers::crates::{crate_file_path, validate_crate_name, validate_version};
use bytes::Bytes;
use quench_http::prelude::{Path, Response, get, http::StatusCode};

#[get("/api/v1/crates/{name}/{version}/download")]
#[tracing::instrument]
pub async fn handle(Path((name, version)): Path<(String, String)>) -> Response {
    if !validate_crate_name(&name) || !validate_version(&version) {
        return not_found();
    }

    let Some(crate_path) = crate_file_path(&name, &version) else {
        return not_found();
    };

    let data = match tokio::fs::read(&crate_path).await {
        Ok(d) => d,
        Err(_) => return not_found(),
    };

    Response::from_bytes(StatusCode::OK, Bytes::from(data))
        .header("content-type", "application/octet-stream")
        .header(
            "content-disposition",
            format!("attachment; filename=\"{name}-{version}.crate\""),
        )
}

fn not_found() -> Response {
    Response::json(
        StatusCode::NOT_FOUND,
        &serde_json::json!({ "errors": [{ "detail": "crate or version not found" }] }),
    )
    .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
}

pub fn register_routes() {
    let _ = handle as fn(_) -> _;
}
