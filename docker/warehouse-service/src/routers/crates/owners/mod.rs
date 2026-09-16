use crate::routers::crates::{crates_storage_root, validate_crate_name};
use quench_http::prelude::{Json, Path, Response, delete, get, http::StatusCode, put};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// On-disk path for a crate's owners file: `<root>/<n>/owners.json`.
fn owners_path(name: &str) -> PathBuf {
    PathBuf::from(crates_storage_root())
        .join(name)
        .join("owners.json")
}

async fn load_owners(name: &str) -> Option<Vec<Owner>> {
    let path = owners_path(name);
    let data = tokio::fs::read(&path).await.ok()?;
    serde_json::from_slice(&data).ok()
}

async fn save_owners(name: &str, owners: &[Owner]) -> std::io::Result<()> {
    let path = owners_path(name);
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let data = serde_json::to_vec_pretty(owners).map_err(std::io::Error::other)?;
    tokio::fs::write(&path, data).await
}

/// Whether the crate has been published.
async fn crate_exists(name: &str) -> bool {
    let path = PathBuf::from(crates_storage_root()).join(name);
    tokio::fs::metadata(&path)
        .await
        .map(|m| m.is_dir())
        .unwrap_or(false)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Owner {
    /// Monotonically assigned on add; informational only for Cargo.
    pub id: u64,
    pub login: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Deserialize)]
pub struct OwnersRequest {
    pub users: Vec<String>,
}

#[derive(Serialize)]
pub struct OwnersResponse {
    pub users: Vec<Owner>,
}

#[derive(Serialize)]
pub struct OkResponse {
    pub ok: bool,
}

#[get("/api/v1/crates/{name}/owners")]
pub async fn list(Path(name): Path<String>) -> Response {
    let name = name.to_ascii_lowercase();

    if !validate_crate_name(&name) {
        return not_found();
    }
    if !crate_exists(&name).await {
        return not_found();
    }

    let owners = load_owners(&name).await.unwrap_or_default();
    Response::json(StatusCode::OK, &OwnersResponse { users: owners })
        .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
}

#[put("/api/v1/crates/{name}/owners")]
pub async fn add(Path(name): Path<String>, Json(body): Json<OwnersRequest>) -> Response {
    let name = name.to_ascii_lowercase();

    if !validate_crate_name(&name) {
        return not_found();
    }
    if !crate_exists(&name).await {
        return not_found();
    }
    if body.users.is_empty() {
        return Response::json(
            StatusCode::BAD_REQUEST,
            &serde_json::json!({ "errors": [{ "detail": "users list must not be empty" }] }),
        )
        .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR));
    }

    let mut owners = load_owners(&name).await.unwrap_or_default();

    let mut next_id = owners.iter().map(|o| o.id).max().unwrap_or(0) + 1;

    for login in &body.users {
        let login = login.trim().to_string();
        if login.is_empty() {
            continue;
        }
        if owners.iter().any(|o| o.login.eq_ignore_ascii_case(&login)) {
            continue;
        }
        owners.push(Owner {
            id: next_id,
            login,
            name: None,
        });
        next_id += 1;
    }

    if let Err(e) = save_owners(&name, &owners).await {
        return Response::json(
            StatusCode::INTERNAL_SERVER_ERROR,
            &serde_json::json!({ "errors": [{ "detail": format!("failed to save owners: {e}") }] }),
        )
        .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR));
    }

    Response::json(StatusCode::OK, &OkResponse { ok: true })
        .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
}

#[delete("/api/v1/crates/{name}/owners")]
pub async fn remove(Path(name): Path<String>, Json(body): Json<OwnersRequest>) -> Response {
    let name = name.to_ascii_lowercase();

    if !validate_crate_name(&name) {
        return not_found();
    }
    if !crate_exists(&name).await {
        return not_found();
    }
    if body.users.is_empty() {
        return Response::json(
            StatusCode::BAD_REQUEST,
            &serde_json::json!({ "errors": [{ "detail": "users list must not be empty" }] }),
        )
        .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR));
    }

    let mut owners = load_owners(&name).await.unwrap_or_default();

    let remove_set: std::collections::HashSet<String> = body
        .users
        .iter()
        .map(|u| u.trim().to_ascii_lowercase())
        .collect();

    owners.retain(|o| !remove_set.contains(&o.login.to_ascii_lowercase()));

    if let Err(e) = save_owners(&name, &owners).await {
        return Response::json(
            StatusCode::INTERNAL_SERVER_ERROR,
            &serde_json::json!({ "errors": [{ "detail": format!("failed to save owners: {e}") }] }),
        )
        .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR));
    }

    Response::json(StatusCode::OK, &OkResponse { ok: true })
        .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
}

fn not_found() -> Response {
    Response::json(
        StatusCode::NOT_FOUND,
        &serde_json::json!({ "errors": [{ "detail": "crate not found" }] }),
    )
    .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
}

pub fn register_routes() {
    let _ = list as fn(_) -> _;
    let _ = add as fn(_, _) -> _;
    let _ = remove as fn(_, _) -> _;
}
