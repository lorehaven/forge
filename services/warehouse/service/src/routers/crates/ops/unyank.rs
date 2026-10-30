use crate::routers::crates::{crate_file_path, validate_crate_name, validate_version};
use quench_http::prelude::{Path, Response, http::StatusCode, put};
use quench_serde::Serialize;

#[derive(Debug, Serialize)]
pub struct OkResponse {
    ok: bool,
}

#[put("/api/v1/crates/{name}/{version}/unyank")]
#[quench_log::instrument]
pub async fn handle(Path((name, version)): Path<(String, String)>) -> Response {
    if !validate_crate_name(&name) || !validate_version(&version) {
        return not_found();
    }

    // Verify the crate tarball exists on disk
    let Some(crate_path) = crate_file_path(&name, &version) else {
        return not_found();
    };
    if tokio::fs::metadata(&crate_path).await.is_err() {
        return not_found();
    }

    match super::yank::set_yanked(&name, &version, false).await {
        Ok(true) => Response::json(StatusCode::OK, &OkResponse { ok: true })
            .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR)),
        Ok(false) => not_found(),
        Err(msg) => Response::json(
            StatusCode::INTERNAL_SERVER_ERROR,
            &quench_serde::json!({ "errors": [{ "detail": msg }] }),
        )
        .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR)),
    }
}

fn not_found() -> Response {
    Response::json(
        StatusCode::NOT_FOUND,
        &quench_serde::json!({ "errors": [{ "detail": "crate or version not found" }] }),
    )
    .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
}

pub fn register_routes() {
    let _ = handle as fn(_) -> _;
}
