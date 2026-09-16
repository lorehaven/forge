use crate::routers::crates::{
    crate_file_path, index_file_path, validate_crate_name, validate_version,
};
use crate::routers::docker::RawBody;
use crate::routers::docker::token::AuthorizationHeader;
use futures_util::StreamExt;
use http_body_util::BodyExt;
use quench_http::prelude::{Response, http::StatusCode, put};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fmt::Write;
use tokio::io::AsyncWriteExt;

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

#[put("/api/v1/crates/new")]
#[tracing::instrument(skip(body))]
pub async fn handle(
    AuthorizationHeader(authorization): AuthorizationHeader,
    RawBody(body): RawBody,
) -> Response {
    let mut body = body.into_data_stream();

    if authorization.is_none() {
        while body.next().await.is_some() {}
        return error_response(StatusCode::UNAUTHORIZED, "missing authorization token");
    }

    let mut buffer = Vec::new();
    while buffer.len() < 4 {
        match body.next().await {
            Some(Ok(chunk)) => buffer.extend_from_slice(&chunk),
            Some(Err(e)) => {
                return error_response(StatusCode::BAD_REQUEST, &e.to_string());
            }
            None => {
                return error_response(StatusCode::BAD_REQUEST, "payload too short");
            }
        }
    }

    let json_len = u32::from_le_bytes(buffer[..4].try_into().unwrap()) as usize;
    while buffer.len() < 4 + json_len + 4 {
        match body.next().await {
            Some(Ok(chunk)) => buffer.extend_from_slice(&chunk),
            Some(Err(e)) => {
                return error_response(StatusCode::BAD_REQUEST, &e.to_string());
            }
            None => {
                return error_response(StatusCode::BAD_REQUEST, "payload truncated (metadata)");
            }
        }
    }

    let json_bytes = &buffer[4..4 + json_len];
    let meta: PublishMetadata = match serde_json::from_slice(json_bytes) {
        Ok(m) => m,
        Err(e) => {
            return error_response(
                StatusCode::BAD_REQUEST,
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

    if !validate_crate_name(&meta.name) {
        return error_response(StatusCode::UNPROCESSABLE_ENTITY, "invalid crate name");
    }
    if !validate_version(&meta.vers) {
        return error_response(StatusCode::UNPROCESSABLE_ENTITY, "invalid version string");
    }

    let Some(crate_path) = crate_file_path(&meta.name, &meta.vers) else {
        return error_response(
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid crate name or version",
        );
    };

    if tokio::fs::metadata(&crate_path).await.is_ok() {
        return error_response(
            StatusCode::CONFLICT,
            "this version has already been published",
        );
    }

    if let Some(parent) = crate_path.parent()
        && tokio::fs::create_dir_all(parent).await.is_err()
    {
        return error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "failed to create storage directory",
        );
    }

    let mut file = match tokio::fs::File::create(&crate_path).await {
        Ok(f) => f,
        Err(e) => {
            tracing::error!("Failed to create crate file {:?}: {}", crate_path, e);
            return error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "failed to create crate file",
            );
        }
    };

    let mut hasher = Sha256::new();
    let mut written_len = 0;

    // Leftover bytes from buffer after the metadata.
    let initial_crate_data = &buffer[crate_len_offset + 4..];
    if !initial_crate_data.is_empty() {
        hasher.update(initial_crate_data);
        if let Err(e) = file.write_all(initial_crate_data).await {
            tracing::error!("Failed to write initial crate data: {}", e);
            return error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "failed to write crate data",
            );
        }
        written_len += initial_crate_data.len();
    }

    while let Some(chunk) = body.next().await {
        let chunk = match chunk {
            Ok(c) => c,
            Err(e) => {
                return error_response(StatusCode::BAD_REQUEST, &e.to_string());
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
                    StatusCode::INTERNAL_SERVER_ERROR,
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
        return error_response(StatusCode::BAD_REQUEST, "payload truncated (crate tarball)");
    }

    if let Err(e) = file.flush().await {
        tracing::error!("Failed to flush crate file: {}", e);
    }

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
                StatusCode::INTERNAL_SERVER_ERROR,
                "failed to serialize index record",
            );
        }
    };

    let Some(index_path) = index_file_path(&meta.name) else {
        return error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "failed to resolve index path",
        );
    };

    if let Some(parent) = index_path.parent()
        && tokio::fs::create_dir_all(parent).await.is_err()
    {
        return error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
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
                StatusCode::INTERNAL_SERVER_ERROR,
                "failed to open index file",
            );
        }
    };

    if let Err(e) = index_file.write_all(record_line.as_bytes()).await {
        tracing::error!("Failed to write to index file {:?}: {}", index_path, e);
        return error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "failed to write index entry",
        );
    }

    Response::json(
        StatusCode::OK,
        &PublishResponse {
            warnings: PublishWarnings {
                invalid_categories: vec![],
                invalid_badges: vec![],
                other: vec![],
            },
        },
    )
    .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
}

fn error_response(status: StatusCode, detail: &str) -> Response {
    tracing::warn!("Crate publish error ({}): {}", status, detail);
    Response::json(
        status,
        &serde_json::json!({ "errors": [{ "detail": detail }] }),
    )
    .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
}

pub fn register_routes() {
    let _ = handle as fn(_, _) -> _;
}
