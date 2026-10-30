use crate::routers::crates::{
    crate_file_path, index_file_path, validate_crate_name, validate_version,
};
use crate::routers::docker::RawBody;
use crate::routers::docker::token::AuthorizationHeader;
use quench_core::stream::StreamExt;
use quench_http::prelude::{Response, http::StatusCode, put};
use quench_serde::{Deserialize, Serialize};
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

#[derive(Debug, Serialize)]
pub struct PublishWarnings {
    invalid_categories: Vec<String>,
    invalid_badges: Vec<String>,
    other: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct PublishResponse {
    warnings: PublishWarnings,
}

/// Reads from `body` into `buffer` until it holds `needed` bytes; `short` says what ran out.
async fn fill<B, E>(
    body: &mut (impl quench_core::stream::Stream<Item = Result<B, E>> + Unpin),
    buffer: &mut Vec<u8>,
    needed: usize,
    short: &str,
) -> Result<(), Response>
where
    B: AsRef<[u8]>,
    E: std::fmt::Display,
{
    while buffer.len() < needed {
        match body.next().await {
            Some(Ok(chunk)) => buffer.extend_from_slice(chunk.as_ref()),
            Some(Err(e)) => return Err(error_response(StatusCode::BAD_REQUEST, &e.to_string())),
            None => return Err(error_response(StatusCode::BAD_REQUEST, short)),
        }
    }
    Ok(())
}

impl IndexRecord {
    /// The sparse-index line for a version just published.
    fn new(meta: PublishMetadata, cksum: String) -> Self {
        let deps = meta
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

        Self {
            name: meta.name,
            vers: meta.vers,
            deps,
            cksum,
            features: meta.features,
            features2: meta.features2,
            yanked: false,
            links: meta.links,
            rust_version: meta.rust_version,
            v: 1,
        }
    }
}

/// Appends one line to the crate's index file, creating it (and its directory) if need be.
async fn append_index(index_path: &std::path::Path, line: &str) -> Result<(), Response> {
    if let Some(parent) = index_path.parent()
        && tokio::fs::create_dir_all(parent).await.is_err()
    {
        return Err(error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "failed to create index directory",
        ));
    }

    let mut index_file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(index_path)
        .await
        .map_err(|e| {
            quench_log::error!("Failed to open index file {:?}: {}", index_path, e);
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "failed to open index file",
            )
        })?;

    index_file.write_all(line.as_bytes()).await.map_err(|e| {
        quench_log::error!("Failed to write to index file {:?}: {}", index_path, e);
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "failed to write index entry",
        )
    })
}

/// Writes the tarball (the bytes already buffered, then the rest of `body`) up to `crate_len`; returns
/// its SHA-256 as hex.
async fn write_crate<B, E>(
    crate_path: &std::path::Path,
    initial_crate_data: &[u8],
    body: &mut (impl quench_core::stream::Stream<Item = Result<B, E>> + Unpin),
    crate_len: usize,
) -> Result<String, Response>
where
    B: AsRef<[u8]>,
    E: std::fmt::Display,
{
    if let Some(parent) = crate_path.parent()
        && tokio::fs::create_dir_all(parent).await.is_err()
    {
        return Err(error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "failed to create storage directory",
        ));
    }

    let mut file = match tokio::fs::File::create(crate_path).await {
        Ok(f) => f,
        Err(e) => {
            quench_log::error!("Failed to create crate file {:?}: {}", crate_path, e);
            return Err(error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "failed to create crate file",
            ));
        }
    };

    let mut hasher = quench_crypto::hash::Hasher::new(quench_crypto::hash::Algorithm::Sha256);
    let mut written_len = 0;

    if !initial_crate_data.is_empty() {
        hasher.update(initial_crate_data);
        if let Err(e) = file.write_all(initial_crate_data).await {
            quench_log::error!("Failed to write initial crate data: {}", e);
            return Err(error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "failed to write crate data",
            ));
        }
        written_len += initial_crate_data.len();
    }

    while let Some(chunk) = body.next().await {
        let chunk = match chunk {
            Ok(c) => c,
            Err(e) => {
                return Err(error_response(StatusCode::BAD_REQUEST, &e.to_string()));
            }
        };

        let chunk = chunk.as_ref();
        let to_write = if written_len + chunk.len() > crate_len {
            &chunk[..crate_len - written_len]
        } else {
            chunk
        };

        if !to_write.is_empty() {
            hasher.update(to_write);
            if let Err(e) = file.write_all(to_write).await {
                quench_log::error!("Failed to write crate chunk: {}", e);
                return Err(error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "failed to write crate chunk",
                ));
            }
            written_len += to_write.len();
        }

        if written_len >= crate_len {
            break;
        }
    }

    if written_len < crate_len {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "payload truncated (crate tarball)",
        ));
    }

    if let Err(e) = file.flush().await {
        quench_log::error!("Failed to flush crate file: {}", e);
    }

    let digest = hasher.finish();
    let mut cksum = String::with_capacity(digest.len() * 2);
    for byte in digest {
        write!(&mut cksum, "{byte:02x}").unwrap();
    }

    Ok(cksum)
}

#[put("/api/v1/crates/new")]
#[quench_log::instrument(skip(body))]
pub async fn handle(
    AuthorizationHeader(authorization): AuthorizationHeader,
    RawBody(body): RawBody,
) -> Response {
    let mut body = body.into_body();

    if authorization.is_none() {
        while body.next().await.is_some() {}
        return error_response(StatusCode::UNAUTHORIZED, "missing authorization token");
    }

    let mut buffer = Vec::new();
    if let Err(response) = fill(&mut body, &mut buffer, 4, "payload too short").await {
        return response;
    }

    let json_len = u32::from_le_bytes(buffer[..4].try_into().unwrap()) as usize;
    if let Err(response) = fill(
        &mut body,
        &mut buffer,
        4 + json_len + 4,
        "payload truncated (metadata)",
    )
    .await
    {
        return response;
    }

    let json_bytes = &buffer[4..4 + json_len];
    let meta: PublishMetadata = match quench_serde::json::from_slice(json_bytes) {
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

    let initial_crate_data = &buffer[crate_len_offset + 4..];
    let cksum = match write_crate(&crate_path, initial_crate_data, &mut body, crate_len).await {
        Ok(cksum) => cksum,
        Err(response) => return response,
    };

    let index_name = meta.name.clone();
    let record = IndexRecord::new(meta, cksum);

    let record_line = format!("{}\n", quench_serde::json::to_string(&record));

    let Some(index_path) = index_file_path(&index_name) else {
        return error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "failed to resolve index path",
        );
    };

    if let Err(response) = append_index(&index_path, &record_line).await {
        return response;
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
    quench_log::warn!("Crate publish error ({}): {}", status, detail);
    Response::json(
        status,
        &quench_serde::json!({ "errors": [{ "detail": detail }] }),
    )
    .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
}

pub fn register_routes() {
    let _ = handle as fn(_, _) -> _;
}
