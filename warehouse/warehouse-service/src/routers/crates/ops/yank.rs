use crate::routers::crates::{
    crate_file_path, index_file_path, validate_crate_name, validate_version,
};
use actix_web::{HttpResponse, Responder, delete, web};
use serde::Serialize;
use utoipa::ToSchema;

#[derive(Serialize, ToSchema)]
pub struct OkResponse {
    ok: bool,
}

#[utoipa::path(
    delete,
    operation_id = "yank_crate",
    tags = ["crates"],
    path = "/{name}/{version}/yank",
    params(
        ("name"    = String, Path, description = "Crate name"),
        ("version" = String, Path, description = "Crate version"),
    ),
    responses(
        (status = 200,  description = "Crate version yanked", body = OkResponse, content_type = "application/json"),
        (status = 401,  description = "Authentication required"),
        (status = 403,  description = "Access denied"),
        (status = 404,  description = "Crate or version not found"),
        (status = 429,  description = "Too many requests"),
    ),
    security(("bearerAuth" = []))
)]
#[delete("/{name}/{version}/yank")]
pub async fn handle(path: web::Path<(String, String)>) -> impl Responder {
    let (name, version) = path.into_inner();

    if !validate_crate_name(&name) || !validate_version(&version) {
        return not_found();
    }

    // Verify the crate file actually exists
    let Some(crate_path) = crate_file_path(&name, &version) else {
        return not_found();
    };
    if tokio::fs::metadata(&crate_path).await.is_err() {
        return not_found();
    }

    match set_yanked(&name, &version, true).await {
        Ok(true) => HttpResponse::Ok().json(OkResponse { ok: true }),
        Ok(false) => not_found(),
        Err(msg) => HttpResponse::InternalServerError().json(serde_json::json!({
            "errors": [{ "detail": msg }]
        })),
    }
}

// ---------------------------------------------------------------------------
// Shared yank helper (also used by unyank)
// ---------------------------------------------------------------------------

/// Rewrites the index file so that the entry for `version` has `yanked` set to
/// `yanked_value`.  Returns `Ok(true)` when the version was found and updated,
/// `Ok(false)` when not found, or `Err(String)` on I/O failures.
pub(in crate::routers::crates) async fn set_yanked(
    name: &str,
    version: &str,
    yanked_value: bool,
) -> Result<bool, String> {
    let Some(index_path) = index_file_path(name) else {
        return Err("failed to resolve index path".into());
    };

    let content = match tokio::fs::read_to_string(&index_path).await {
        Ok(s) => s,
        Err(_) => return Ok(false), // index file doesn't exist → version not found
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
            Err(_) => {
                // Preserve malformed lines as-is
                new_lines.push(trimmed.to_string());
            }
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

fn not_found() -> HttpResponse {
    HttpResponse::NotFound().json(serde_json::json!({
        "errors": [{ "detail": "crate or version not found" }]
    }))
}
