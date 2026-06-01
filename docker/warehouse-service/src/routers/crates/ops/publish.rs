use crate::routers::crates::{
    crate_file_path, index_file_path, validate_crate_name, validate_version,
};
use actix_web::{HttpResponse, Responder, put, web};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fmt::Write;
use tokio::io::AsyncWriteExt;

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

#[derive(Serialize)]
pub struct PublishWarnings {
    invalid_categories: Vec<String>,
    invalid_badges: Vec<String>,
    other: Vec<String>,
}

#[derive(Serialize)]
pub struct PublishResponse {
    warnings: PublishWarnings,
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

#[put("/new")]
#[tracing::instrument(skip(body))]
pub async fn handle(req: actix_web::HttpRequest, mut body: web::Payload) -> impl Responder {
    if req.headers().get("Authorization").is_none() {
        while body.next().await.is_some() {}
        return error_response(
            actix_web::http::StatusCode::UNAUTHORIZED,
            "missing authorization token",
        );
    }

    // ------------------------------------------------------------------
    // 1. Collect initial bytes to parse JSON length
    // ------------------------------------------------------------------
    let mut buffer = Vec::new();
    while buffer.len() < 4 {
        match body.next().await {
            Some(Ok(chunk)) => buffer.extend_from_slice(&chunk),
            Some(Err(e)) => {
                return error_response(actix_web::http::StatusCode::BAD_REQUEST, &e.to_string());
            }
            None => {
                return error_response(
                    actix_web::http::StatusCode::BAD_REQUEST,
                    "payload too short",
                );
            }
        }
    }

    let json_len = u32::from_le_bytes(buffer[..4].try_into().unwrap()) as usize;
    while buffer.len() < 4 + json_len + 4 {
        match body.next().await {
            Some(Ok(chunk)) => buffer.extend_from_slice(&chunk),
            Some(Err(e)) => {
                return error_response(actix_web::http::StatusCode::BAD_REQUEST, &e.to_string());
            }
            None => {
                return error_response(
                    actix_web::http::StatusCode::BAD_REQUEST,
                    "payload truncated (metadata)",
                );
            }
        }
    }

    let json_bytes = &buffer[4..4 + json_len];
    let meta: PublishMetadata = match serde_json::from_slice(json_bytes) {
        Ok(m) => m,
        Err(e) => {
            return error_response(
                actix_web::http::StatusCode::BAD_REQUEST,
                &format!("invalid metadata JSON: {e}"),
            );
        }
    };

    let crate_len_offset = 4 + json_len;
    let crate_len = u32::from_le_bytes(
        buffer[crate_len_offset..crate_len_offset + 4]
            .try_into()
            .unwrap(),
    ) as usize;

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
    // 3. Persist .crate tarball and compute SHA-256 incrementally
    // ------------------------------------------------------------------
    if let Some(parent) = crate_path.parent()
        && tokio::fs::create_dir_all(parent).await.is_err()
    {
        return error_response(
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            "failed to create storage directory",
        );
    }

    let mut file = match tokio::fs::File::create(&crate_path).await {
        Ok(f) => f,
        Err(e) => {
            tracing::error!("Failed to create crate file {:?}: {}", crate_path, e);
            return error_response(
                actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                "failed to create crate file",
            );
        }
    };

    let mut hasher = Sha256::new();
    let mut written_len = 0;

    // Write any leftover bytes from buffer after the metadata
    let initial_crate_data = &buffer[crate_len_offset + 4..];
    if !initial_crate_data.is_empty() {
        hasher.update(initial_crate_data);
        if let Err(e) = file.write_all(initial_crate_data).await {
            tracing::error!("Failed to write initial crate data: {}", e);
            return error_response(
                actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                "failed to write crate data",
            );
        }
        written_len += initial_crate_data.len();
    }

    // Stream the rest of the body
    while let Some(chunk) = body.next().await {
        let chunk = match chunk {
            Ok(c) => c,
            Err(e) => {
                return error_response(actix_web::http::StatusCode::BAD_REQUEST, &e.to_string());
            }
        };

        let to_write = if written_len + chunk.len() > crate_len {
            &chunk[..crate_len - written_len]
        } else {
            &chunk
        };

        if !to_write.is_empty() {
            hasher.update(to_write);
            if let Err(e) = file.write_all(to_write).await {
                tracing::error!("Failed to write crate chunk: {}", e);
                return error_response(
                    actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                    "failed to write crate chunk",
                );
            }
            written_len += to_write.len();
        }

        if written_len >= crate_len {
            break;
        }
    }

    if written_len < crate_len {
        return error_response(
            actix_web::http::StatusCode::BAD_REQUEST,
            "payload truncated (crate tarball)",
        );
    }

    if let Err(e) = file.flush().await {
        tracing::error!("Failed to flush crate file: {}", e);
    }

    // ------------------------------------------------------------------
    // 4. Finalize checksum and build index record
    // ------------------------------------------------------------------
    let digest = hasher.finalize();
    let mut cksum = String::with_capacity(digest.len() * 2);
    for byte in digest {
        write!(&mut cksum, "{:02x}", byte).unwrap();
    }

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
    // 5. Append to sparse index file
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

    let mut index_file = match tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&index_path)
        .await
    {
        Ok(f) => f,
        Err(e) => {
            tracing::error!("Failed to open index file {:?}: {}", index_path, e);
            return error_response(
                actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                "failed to open index file",
            );
        }
    };

    if let Err(e) = index_file.write_all(record_line.as_bytes()).await {
        tracing::error!("Failed to write to index file {:?}: {}", index_path, e);
        return error_response(
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            "failed to write index entry",
        );
    }

    // ------------------------------------------------------------------
    // 6. Respond
    // ------------------------------------------------------------------
    HttpResponse::Ok().json(PublishResponse {
        warnings: PublishWarnings {
            invalid_categories: vec![],
            invalid_badges: vec![],
            other: vec![],
        },
    })
}

fn error_response(status: actix_web::http::StatusCode, detail: &str) -> HttpResponse {
    tracing::warn!("Crate publish error ({}): {}", status, detail);
    HttpResponse::build(status).json(serde_json::json!({
        "errors": [{ "detail": detail }]
    }))
}
