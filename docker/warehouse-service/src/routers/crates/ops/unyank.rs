use crate::routers::crates::{crate_file_path, validate_crate_name, validate_version};
use actix_web::{HttpResponse, Responder, put, web};
use serde::Serialize;
use utoipa::ToSchema;

#[derive(Serialize, ToSchema)]
pub struct OkResponse {
    ok: bool,
}

#[utoipa::path(
    put,
    operation_id = "unyank_crate",
    tags = ["crates"],
    path = "/{name}/{version}/unyank",
    params(
        ("name"    = String, Path, description = "Crate name"),
        ("version" = String, Path, description = "Crate version"),
    ),
    responses(
        (status = 200,  description = "Crate version unyanked", body = OkResponse, content_type = "application/json"),
        (status = 401,  description = "Authentication required"),
        (status = 403,  description = "Access denied"),
        (status = 404,  description = "Crate or version not found"),
        (status = 429,  description = "Too many requests"),
    ),
    security(("bearerAuth" = []))
)]
#[put("/{name}/{version}/unyank")]
pub async fn handle(path: web::Path<(String, String)>) -> impl Responder {
    let (name, version) = path.into_inner();

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
        Ok(true) => HttpResponse::Ok().json(OkResponse { ok: true }),
        Ok(false) => not_found(),
        Err(msg) => HttpResponse::InternalServerError().json(serde_json::json!({
            "errors": [{ "detail": msg }]
        })),
    }
}

fn not_found() -> HttpResponse {
    HttpResponse::NotFound().json(serde_json::json!({
        "errors": [{ "detail": "crate or version not found" }]
    }))
}
