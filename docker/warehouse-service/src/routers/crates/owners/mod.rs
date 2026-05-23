use crate::routers::crates::{CRATES_STORAGE_ROOT, validate_crate_name};
use actix_web::{HttpResponse, Responder, delete, get, put, web};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use utoipa::ToSchema;

// ---------------------------------------------------------------------------
// Storage helper
// ---------------------------------------------------------------------------

/// On-disk path for a crate's owners file.
///
/// Layout: `<root>/<n>/owners.json`
fn owners_path(name: &str) -> PathBuf {
    PathBuf::from(CRATES_STORAGE_ROOT.as_str())
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
    // Ensure the crate directory exists
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let data = serde_json::to_vec_pretty(owners).map_err(std::io::Error::other)?;
    tokio::fs::write(&path, data).await
}

/// Returns `true` if the crate directory exists (i.e. the crate has been published).
async fn crate_exists(name: &str) -> bool {
    let path = PathBuf::from(CRATES_STORAGE_ROOT.as_str()).join(name);
    tokio::fs::metadata(&path)
        .await
        .map(|m| m.is_dir())
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Shared types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Owner {
    /// Numeric id (monotonically assigned on add; stable for the lifetime of
    /// the entry – purely informational for Cargo).
    pub id: u64,
    /// The login / username string Cargo uses to identify the owner.
    pub login: String,
    /// Optional human-readable display name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct OwnersRequest {
    /// List of login names to add or remove.
    pub users: Vec<String>,
}

#[derive(Serialize, ToSchema)]
pub struct OwnersResponse {
    pub users: Vec<Owner>,
}

#[derive(Serialize, ToSchema)]
pub struct OkResponse {
    pub ok: bool,
}

// ---------------------------------------------------------------------------
// GET /api/v1/crates/{name}/owners
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    operation_id = "list_owners",
    tags = ["crates - owners"],
    path = "/{name}/owners",
    params(
        ("name" = String, Path, description = "Crate name"),
    ),
    responses(
        (status = 200, description = "Owner list", body = OwnersResponse, content_type = "application/json"),
        (status = 401, description = "Authentication required"),
        (status = 404, description = "Crate not found"),
    ),
    security(("bearerAuth" = []))
)]
#[get("/{name}/owners")]
pub async fn list(path: web::Path<String>) -> impl Responder {
    let name = path.into_inner().to_ascii_lowercase();

    if !validate_crate_name(&name) {
        return not_found();
    }
    if !crate_exists(&name).await {
        return not_found();
    }

    let owners = load_owners(&name).await.unwrap_or_default();
    HttpResponse::Ok().json(OwnersResponse { users: owners })
}

// ---------------------------------------------------------------------------
// PUT /api/v1/crates/{name}/owners
// ---------------------------------------------------------------------------

#[utoipa::path(
    put,
    operation_id = "add_owners",
    tags = ["crates - owners"],
    path = "/{name}/owners",
    params(
        ("name" = String, Path, description = "Crate name"),
    ),
    request_body(
        content = OwnersRequest,
        content_type = "application/json",
        description = "List of login names to add",
    ),
    responses(
        (status = 200, description = "Owners added", body = OkResponse, content_type = "application/json"),
        (status = 400, description = "Bad request"),
        (status = 401, description = "Authentication required"),
        (status = 403, description = "Access denied"),
        (status = 404, description = "Crate not found"),
    ),
    security(("bearerAuth" = []))
)]
#[put("/{name}/owners")]
pub async fn add(path: web::Path<String>, body: web::Json<OwnersRequest>) -> impl Responder {
    let name = path.into_inner().to_ascii_lowercase();

    if !validate_crate_name(&name) {
        return not_found();
    }
    if !crate_exists(&name).await {
        return not_found();
    }
    if body.users.is_empty() {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "errors": [{ "detail": "users list must not be empty" }]
        }));
    }

    let mut owners = load_owners(&name).await.unwrap_or_default();

    // Assign IDs sequentially based on current max; keeps IDs stable for
    // existing entries.
    let mut next_id = owners.iter().map(|o| o.id).max().unwrap_or(0) + 1;

    for login in &body.users {
        let login = login.trim().to_string();
        if login.is_empty() {
            continue;
        }
        // Skip if already an owner (case-insensitive)
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
        return HttpResponse::InternalServerError().json(serde_json::json!({
            "errors": [{ "detail": format!("failed to save owners: {e}") }]
        }));
    }

    HttpResponse::Ok().json(OkResponse { ok: true })
}

// ---------------------------------------------------------------------------
// DELETE /api/v1/crates/{name}/owners
// ---------------------------------------------------------------------------

#[utoipa::path(
    delete,
    operation_id = "remove_owners",
    tags = ["crates - owners"],
    path = "/{name}/owners",
    params(
        ("name" = String, Path, description = "Crate name"),
    ),
    request_body(
        content = OwnersRequest,
        content_type = "application/json",
        description = "List of login names to remove",
    ),
    responses(
        (status = 200, description = "Owners removed", body = OkResponse, content_type = "application/json"),
        (status = 400, description = "Bad request"),
        (status = 401, description = "Authentication required"),
        (status = 403, description = "Access denied"),
        (status = 404, description = "Crate not found"),
    ),
    security(("bearerAuth" = []))
)]
#[delete("/{name}/owners")]
pub async fn remove(path: web::Path<String>, body: web::Json<OwnersRequest>) -> impl Responder {
    let name = path.into_inner().to_ascii_lowercase();

    if !validate_crate_name(&name) {
        return not_found();
    }
    if !crate_exists(&name).await {
        return not_found();
    }
    if body.users.is_empty() {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "errors": [{ "detail": "users list must not be empty" }]
        }));
    }

    let mut owners = load_owners(&name).await.unwrap_or_default();

    let remove_set: std::collections::HashSet<String> = body
        .users
        .iter()
        .map(|u| u.trim().to_ascii_lowercase())
        .collect();

    owners.retain(|o| !remove_set.contains(&o.login.to_ascii_lowercase()));

    if let Err(e) = save_owners(&name, &owners).await {
        return HttpResponse::InternalServerError().json(serde_json::json!({
            "errors": [{ "detail": format!("failed to save owners: {e}") }]
        }));
    }

    HttpResponse::Ok().json(OkResponse { ok: true })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn not_found() -> HttpResponse {
    HttpResponse::NotFound().json(serde_json::json!({
        "errors": [{ "detail": "crate not found" }]
    }))
}
