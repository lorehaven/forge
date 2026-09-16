use crate::routers::crates::{
    crate_file_path, index_file_path, validate_crate_name, validate_version,
};
use quench_http::prelude::{Path, Response, delete, http::StatusCode};
use serde::Serialize;

#[derive(Serialize)]
pub struct OkResponse {
    ok: bool,
}

#[delete("/api/v1/crates/{name}/{version}/yank")]
#[tracing::instrument]
pub async fn handle(Path((name, version)): Path<(String, String)>) -> Response {
    if !validate_crate_name(&name) || !validate_version(&version) {
        return not_found();
    }

    let Some(crate_path) = crate_file_path(&name, &version) else {
        return not_found();
    };
    if tokio::fs::metadata(&crate_path).await.is_err() {
        return not_found();
    }

    match set_yanked(&name, &version, true).await {
        Ok(true) => Response::json(StatusCode::OK, &OkResponse { ok: true })
            .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR)),
        Ok(false) => not_found(),
        Err(msg) => Response::json(
            StatusCode::INTERNAL_SERVER_ERROR,
            &serde_json::json!({ "errors": [{ "detail": msg }] }),
        )
        .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR)),
    }
}

/// Sets `yanked` for `version`'s index entry; also used by unyank.
#[tracing::instrument]
pub async fn set_yanked(name: &str, version: &str, yanked_value: bool) -> Result<bool, String> {
    let Some(index_path) = index_file_path(name) else {
        return Err("failed to resolve index path".into());
    };

    let content = match tokio::fs::read_to_string(&index_path).await {
        Ok(s) => s,
        Err(_) => return Ok(false), // no index file - version not found
    };

    let mut found = false;
    let mut new_lines: Vec<String> = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            new_lines.push(String::new());
            continue;
        }

        match serde_json::from_str::<serde_json::Value>(trimmed) {
            Ok(mut record) => {
                if record.get("vers").and_then(|v| v.as_str()) == Some(version) {
                    found = true;
                    record["yanked"] = serde_json::Value::Bool(yanked_value);
                }
                match serde_json::to_string(&record) {
                    Ok(s) => new_lines.push(s),
                    Err(e) => return Err(format!("failed to serialize index record: {e}")),
                }
            }
            Err(_) => new_lines.push(trimmed.to_string()), // preserve malformed lines as-is
        }
    }

    if !found {
        return Ok(false);
    }

    let new_content = new_lines.join("\n") + "\n";
    tokio::fs::write(&index_path, new_content.as_bytes())
        .await
        .map_err(|e| format!("failed to write index file: {e}"))?;

    Ok(true)
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
