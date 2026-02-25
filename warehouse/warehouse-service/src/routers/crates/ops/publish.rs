use crate::routers::crates::{
    crate_file_path, index_file_path, validate_crate_name, validate_version,
};
use actix_web::{HttpResponse, Responder, put, web};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use utoipa::ToSchema;

// ---------------------------------------------------------------------------
// Wire-format structs (cargo publish binary payload → metadata JSON)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct PublishMetadata {
    name: String,
    vers: String,
    deps: Vec<PublishDep>,
    features: HashMap<String, Vec<String>>,
    #[serde(default)]
    features2: Option<HashMap<String, Vec<String>>>,
    #[serde(default)]
    links: Option<String>,
    #[serde(default)]
    rust_version: Option<String>,
    // Extra fields cargo may send, but we don't need to store in the index
    // description, homepage, etc. – we simply ignore them here.
}

#[derive(Debug, Deserialize, Serialize)]
struct PublishDep {
    name: String,
    version_req: String,
    features: Vec<String>,
    optional: bool,
    default_features: bool,
    target: Option<String>,
    kind: String,
    #[serde(default)]
    registry: Option<String>,
    #[serde(default)]
    explicit_name_in_toml: Option<String>,
}

// ---------------------------------------------------------------------------
// Index record (newline-delimited JSON written to the sparse index)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct IndexRecord {
    name: String,
    vers: String,
    deps: Vec<IndexDep>,
    cksum: String,
    features: HashMap<String, Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    features2: Option<HashMap<String, Vec<String>>>,
    yanked: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    links: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rust_version: Option<String>,
    v: u8,
}

#[derive(Debug, Serialize)]
struct IndexDep {
    name: String,
    req: String,
    features: Vec<String>,
    optional: bool,
    default_features: bool,
    target: Option<String>,
    kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    registry: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    package: Option<String>,
}

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

#[derive(Serialize, ToSchema)]
pub struct PublishWarnings {
    invalid_categories: Vec<String>,
    invalid_badges: Vec<String>,
    other: Vec<String>,
}

#[derive(Serialize, ToSchema)]
pub struct PublishResponse {
    warnings: PublishWarnings,
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

#[utoipa::path(
    put,
    operation_id = "publish_crate",
    tags = ["crates"],
    path = "/new",
    request_body(
        content = Vec<u8>,
        content_type = "application/octet-stream",
        description = "Cargo publish binary payload: u32LE metadata-len, JSON metadata, u32LE crate-len, .crate tarball",
    ),
    responses(
        (status = 200,  description = "Crate published successfully",    body = PublishResponse, content_type = "application/json"),
        (status = 400,  description = "Bad request / malformed payload"),
        (status = 401,  description = "Authentication required"),
        (status = 403,  description = "Access denied"),
        (status = 409,  description = "Version already exists"),
        (status = 422,  description = "Validation error"),
        (status = 429,  description = "Too many requests"),
    ),
    security(("bearerAuth" = []))
)]
#[put("/new")]
pub async fn handle(body: web::Bytes) -> impl Responder {
    // ------------------------------------------------------------------
    // 1. Parse the cargo binary wire format
    //    [ u32LE json_len ][ json bytes ][ u32LE crate_len ][ crate bytes ]
    // ------------------------------------------------------------------
    let (meta, crate_bytes) = match parse_publish_body(&body) {
        Ok(v) => v,
        Err(msg) => {
            return error_response(actix_web::http::StatusCode::BAD_REQUEST, &msg);
        }
    };

    // ------------------------------------------------------------------
    // 2. Validate name & version
    // ------------------------------------------------------------------
    if !validate_crate_name(&meta.name) {
        return error_response(
            actix_web::http::StatusCode::UNPROCESSABLE_ENTITY,
            "invalid crate name",
        );
    }
    if !validate_version(&meta.vers) {
        return error_response(
            actix_web::http::StatusCode::UNPROCESSABLE_ENTITY,
            "invalid version string",
        );
    }

    // ------------------------------------------------------------------
    // 3. Reject if already published
    // ------------------------------------------------------------------
    let Some(crate_path) = crate_file_path(&meta.name, &meta.vers) else {
        return error_response(
            actix_web::http::StatusCode::UNPROCESSABLE_ENTITY,
            "invalid crate name or version",
        );
    };

    if tokio::fs::metadata(&crate_path).await.is_ok() {
        return error_response(
            actix_web::http::StatusCode::CONFLICT,
            "this version has already been published",
        );
    }

    // ------------------------------------------------------------------
    // 4. Persist .crate tarball
    // ------------------------------------------------------------------
    if let Some(parent) = crate_path.parent()
        && tokio::fs::create_dir_all(parent).await.is_err()
    {
        return error_response(
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            "failed to create storage directory",
        );
    }

    if tokio::fs::write(&crate_path, &crate_bytes).await.is_err() {
        return error_response(
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            "failed to write crate file",
        );
    }

    // ------------------------------------------------------------------
    // 5. Compute SHA-256 checksum
    // ------------------------------------------------------------------
    let cksum = {
        let mut hasher = Sha256::new();
        hasher.update(crate_bytes);
        format!("{:x}", hasher.finalize())
    };

    // ------------------------------------------------------------------
    // 6. Build index record
    // ------------------------------------------------------------------
    let index_deps: Vec<IndexDep> = meta
        .deps
        .into_iter()
        .map(|d| {
            let package = d.explicit_name_in_toml.filter(|p| p != &d.name);
            IndexDep {
                name: d.name,
                req: d.version_req,
                features: d.features,
                optional: d.optional,
                default_features: d.default_features,
                target: d.target,
                kind: d.kind,
                registry: d.registry,
                package,
            }
        })
        .collect();

    let record = IndexRecord {
        name: meta.name.clone(),
        vers: meta.vers.clone(),
        deps: index_deps,
        cksum,
        features: meta.features,
        features2: meta.features2,
        yanked: false,
        links: meta.links,
        rust_version: meta.rust_version,
        v: 1,
    };

    let record_line = match serde_json::to_string(&record) {
        Ok(s) => format!("{s}\n"),
        Err(_) => {
            return error_response(
                actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                "failed to serialize index record",
            );
        }
    };

    // ------------------------------------------------------------------
    // 7. Append to sparse index file
    // ------------------------------------------------------------------
    let Some(index_path) = index_file_path(&meta.name) else {
        return error_response(
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            "failed to resolve index path",
        );
    };

    if let Some(parent) = index_path.parent()
        && tokio::fs::create_dir_all(parent).await.is_err()
    {
        return error_response(
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            "failed to create index directory",
        );
    }

    use tokio::io::AsyncWriteExt;
    match tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&index_path)
        .await
    {
        Ok(mut file) => {
            if file.write_all(record_line.as_bytes()).await.is_err() {
                return error_response(
                    actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                    "failed to write index entry",
                );
            }
        }
        Err(_) => {
            return error_response(
                actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                "failed to open index file",
            );
        }
    }

    // ------------------------------------------------------------------
    // 8. Respond
    // ------------------------------------------------------------------
    HttpResponse::Ok().json(PublishResponse {
        warnings: PublishWarnings {
            invalid_categories: vec![],
            invalid_badges: vec![],
            other: vec![],
        },
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn parse_publish_body(body: &[u8]) -> Result<(PublishMetadata, &[u8]), String> {
    if body.len() < 4 {
        return Err("payload too short".into());
    }
    let json_len = u32::from_le_bytes(body[..4].try_into().unwrap()) as usize;
    if body.len() < 4 + json_len + 4 {
        return Err("payload truncated (metadata)".into());
    }
    let json_bytes = &body[4..4 + json_len];
    let meta: PublishMetadata =
        serde_json::from_slice(json_bytes).map_err(|e| format!("invalid metadata JSON: {e}"))?;

    let crate_offset = 4 + json_len;
    let crate_len =
        u32::from_le_bytes(body[crate_offset..crate_offset + 4].try_into().unwrap()) as usize;
    let crate_start = crate_offset + 4;

    if body.len() < crate_start + crate_len {
        return Err("payload truncated (crate tarball)".into());
    }
    let crate_bytes = &body[crate_start..crate_start + crate_len];

    Ok((meta, crate_bytes))
}

fn error_response(status: actix_web::http::StatusCode, detail: &str) -> HttpResponse {
    HttpResponse::build(status).json(serde_json::json!({
        "errors": [{ "detail": detail }]
    }))
}
