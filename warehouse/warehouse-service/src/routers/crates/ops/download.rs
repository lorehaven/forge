use crate::routers::crates::{crate_file_path, validate_crate_name, validate_version};
use actix_web::{HttpResponse, Responder, get, web};

#[utoipa::path(
    get,
    operation_id = "download_crate",
    tags = ["crates"],
    path = "/{name}/{version}/download",
    params(
        ("name"    = String, Path, description = "Crate name"),
        ("version" = String, Path, description = "Crate version"),
    ),
    responses(
        (status = 200,  description = ".crate tarball bytes",       content_type = "application/octet-stream"),
        (status = 401,  description = "Authentication required"),
        (status = 403,  description = "Access denied"),
        (status = 404,  description = "Crate or version not found"),
        (status = 429,  description = "Too many requests"),
    )
)]
#[get("/{name}/{version}/download")]
pub async fn handle(path: web::Path<(String, String)>) -> impl Responder {
    let (name, version) = path.into_inner();

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

    HttpResponse::Ok()
        .content_type("application/octet-stream")
        .append_header(("Content-Length", data.len()))
        .append_header((
            "Content-Disposition",
            format!("attachment; filename=\"{name}-{version}.crate\""),
        ))
        .body(data)
}

fn not_found() -> HttpResponse {
    HttpResponse::NotFound().json(serde_json::json!({
        "errors": [{ "detail": "crate or version not found" }]
    }))
}
