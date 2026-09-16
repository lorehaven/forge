use crate::routers::crates::{index_file_path, index_prefix, validate_crate_name};
use crate::utils::sha256::sha256_hex;
use async_trait::async_trait;
use bytes::Bytes;
use quench_http::prelude::{
    FromRequest, HttpError, Path, Request, Response, get, http::StatusCode,
};
use quench_starter::common::routes::with_base_path;
use serde::Serialize;
use std::sync::LazyLock;

static REGISTRY_BASE_URL: LazyLock<String> =
    LazyLock::new(|| envmnt::get_or("REGISTRY_BASE_URL", "https://localhost"));

#[derive(Serialize)]
struct IndexConfig {
    /// Base URL Cargo appends `/{crate}/{version}/download` to.
    dl: String,
    api: String,
    #[serde(rename = "auth-required")]
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    auth_required: bool,
}

fn index_config_response() -> Response {
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
    Response::json(StatusCode::OK, &config)
        .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
}

/// The `If-None-Match` header, for conditional-GET support.
struct IfNoneMatch(Option<String>);

#[async_trait]
impl FromRequest for IfNoneMatch {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
        Ok(Self(req.header("if-none-match").map(str::to_string)))
    }
}

// One handler for `/index/{path:.*}` special-casing `config.json` internally, since a separate
// literal route would ambiguously overlap the wildcard in link-order-dependent registration order.
#[get("/index/{path:.*}")]
async fn get_crate_index(
    Path(full_path): Path<String>,
    IfNoneMatch(if_none_match): IfNoneMatch,
) -> Response {
    if full_path == "config.json" {
        return index_config_response();
    }

    let (prefix, name) = match full_path.rsplit_once('/') {
        Some((p, n)) => (p, n.to_ascii_lowercase()),
        None => return Response::new(StatusCode::NOT_FOUND),
    };

    // The prefix must match what we'd compute - prevents traversal via a crafted prefix.
    if !validate_crate_name(&name) {
        return Response::new(StatusCode::NOT_FOUND);
    }
    if prefix != index_prefix(&name) {
        return Response::new(StatusCode::NOT_FOUND);
    }

    let Some(index_path) = index_file_path(&name) else {
        return Response::new(StatusCode::NOT_FOUND);
    };

    let data = match tokio::fs::read(&index_path).await {
        Ok(d) => d,
        Err(_) => return Response::new(StatusCode::NOT_FOUND),
    };

    // ETag based on SHA-256 of the file contents
    let etag = format!("sha256:{}", sha256_hex(&data));

    // Conditional GET support
    if let Some(inm) = if_none_match
        && inm == etag
    {
        return Response::new(StatusCode::NOT_MODIFIED).header("etag", &etag);
    }

    Response::from_bytes(StatusCode::OK, Bytes::from(data))
        .header("content-type", "text/plain; charset=utf-8")
        .header("etag", &etag)
        .header("cache-control", "no-cache")
}

pub fn register_routes() {
    let _ = get_crate_index as fn(_, _) -> _;
}
