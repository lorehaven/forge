use crate::routers::crates::{index_file_path, index_prefix, validate_crate_name};
use crate::utils::sha256::sha256_hex;
use actix_web::{HttpRequest, HttpResponse, Responder, get, web};
use quench_starter::prelude::with_base_path;
use serde::Serialize;
use std::sync::LazyLock;
// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

static REGISTRY_BASE_URL: LazyLock<String> =
    LazyLock::new(|| envmnt::get_or("REGISTRY_BASE_URL", "https://localhost"));

#[derive(Serialize)]
struct IndexConfig {
    /// Download URL template (or base URL Cargo appends `/{crate}/{version}/download` to)
    dl: String,
    /// Registry API base URL
    api: String,
    #[serde(rename = "auth-required")]
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    auth_required: bool,
}

#[get("/config.json")]
async fn get_index_config() -> impl Responder {
    let base = format!(
        "{}{}",
        REGISTRY_BASE_URL.as_str().trim_end_matches('/'),
        with_base_path("")
    );
    let config = IndexConfig {
        dl: format!("{base}/api/v1/crates/{{crate}}/{{version}}/download"),
        api: base.to_string(),
        auth_required: false,
    };
    HttpResponse::Ok()
        .content_type("application/json")
        .json(config)
}

// ---------------------------------------------------------------------------
// Per-crate index
// ---------------------------------------------------------------------------

#[get("/{path:.*}")]
async fn get_crate_index(req: HttpRequest, path: web::Path<String>) -> impl Responder {
    let full_path = path.into_inner();

    let (prefix, name) = match full_path.rsplit_once('/') {
        Some((p, n)) => (p, n.to_ascii_lowercase()),
        None => return HttpResponse::NotFound().finish(),
    };

    // Validate name and make sure the prefix the client sent actually matches
    // what we would compute (prevents traversal via crafted prefix).
    if !validate_crate_name(&name) {
        return HttpResponse::NotFound().finish();
    }
    if prefix != index_prefix(&name) {
        return HttpResponse::NotFound().finish();
    }

    let Some(index_path) = index_file_path(&name) else {
        return HttpResponse::NotFound().finish();
    };

    let data = match tokio::fs::read(&index_path).await {
        Ok(d) => d,
        Err(_) => return HttpResponse::NotFound().finish(),
    };

    // ETag based on SHA-256 of the file contents
    let etag = format!("sha256:{}", sha256_hex(&data));

    // Conditional GET support
    if let Some(inm) = req
        .headers()
        .get("If-None-Match")
        .and_then(|v| v.to_str().ok())
        && inm == etag
    {
        return HttpResponse::NotModified()
            .append_header(("ETag", etag))
            .finish();
    }

    HttpResponse::Ok()
        .content_type("text/plain; charset=utf-8")
        .append_header(("ETag", etag))
        .append_header(("Cache-Control", "no-cache"))
        .body(data)
}
