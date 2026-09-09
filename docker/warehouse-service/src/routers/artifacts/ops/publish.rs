//! `PUT /api/v1/artifacts/{program}/{platform}/{version_code}` - publish a
//! build.
//!
//! The body is the raw artifact file, nothing else - there is no separate
//! metadata payload. What the URL asserts (`{program}`, `{platform}`,
//! `{version_code}`) is checked one of two ways:
//!
//! * `android` - decoded from the archive's own `AndroidManifest.xml` and
//!   rejected with `422` if the decoded package/version doesn't match the URL;
//! * everything else - trusted from the URL, since there is no manifest this
//!   service can read. `?format=` is then required, and `?version_name=`,
//!   `?label=`, `?arch=` and `?filename=` are taken as given.
//!
//! Either way `size_bytes` and `sha256` are computed from the stream, and a
//! `(program, platform, version_code)` that already exists is a `409`.

use crate::domain::apk_manifest::{self, ApkManifestError};
use crate::domain::artifact::{ArtifactMetadata, ArtifactVersion, Platform};
use crate::routers::artifacts::ops::{ArtifactView, actor, disabled, error, not_found};
use crate::routers::artifacts::{
    artifact_file_path, artifact_staging_path, default_filename, validate_filename,
    validate_program,
};
use actix_web::http::StatusCode;
use actix_web::{HttpRequest, HttpResponse, Responder, put, web};
use chrono::Utc;
use futures_util::StreamExt;
use quench_db::prelude::{Crud, Db};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use sqlx::types::Json;
use tokio::io::AsyncWriteExt;

/// Query parameters. All optional at the type level; `format` is required for
/// non-android platforms and that is enforced in [`publish`].
#[derive(Debug, Default, Deserialize)]
pub struct PublishParams {
    pub format: Option<String>,
    pub arch: Option<String>,
    pub version_name: Option<String>,
    pub label: Option<String>,
    pub filename: Option<String>,
}

#[put("/{program}/{platform}/{version_code}")]
#[tracing::instrument(skip(body, request))]
pub async fn handle(
    request: HttpRequest,
    db: web::Data<Db>,
    path: web::Path<(String, String, i64)>,
    params: web::Query<PublishParams>,
    mut body: web::Payload,
) -> impl Responder {
    let (program, platform_raw, version_code) = path.into_inner();
    let Some(platform) = Platform::parse(&platform_raw) else {
        return error(StatusCode::UNPROCESSABLE_ENTITY, "unknown platform");
    };
    publish(
        request,
        db.get_ref(),
        program,
        platform,
        version_code,
        params.into_inner(),
        &mut body,
    )
    .await
}

/// The publish itself, once the platform has been resolved. Shared with the
/// `/api/v1/apk` alias (`super::super::alias`), which calls it with
/// [`Platform::Android`].
pub async fn publish(
    request: HttpRequest,
    db: &Db,
    program: String,
    platform: Platform,
    version_code: i64,
    params: PublishParams,
    body: &mut web::Payload,
) -> HttpResponse {
    if !crate::routers::artifacts_enabled() {
        return disabled();
    }

    if !validate_program(&program) {
        return error(StatusCode::UNPROCESSABLE_ENTITY, "invalid program name");
    }
    if version_code <= 0 {
        return error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "version_code must be a positive integer",
        );
    }

    // Resolve the stored `format` before touching disk.
    let format = if platform == Platform::Android {
        "apk".to_string()
    } else {
        match params.format.as_deref().map(str::trim) {
            Some(raw) if is_format_token(raw) => raw.to_ascii_lowercase(),
            Some(_) => {
                return error(StatusCode::UNPROCESSABLE_ENTITY, "invalid format");
            }
            None => {
                return error(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "format is required for non-android artifacts",
                );
            }
        }
    };

    let filename = match params.filename.as_deref().map(str::trim) {
        Some(name) if !name.is_empty() => {
            if !validate_filename(name) {
                return error(StatusCode::UNPROCESSABLE_ENTITY, "invalid filename");
            }
            name.to_string()
        }
        _ => default_filename(&program, version_code, &format),
    };

    let id = ArtifactVersion::id_for(&program, platform, version_code);
    let repo = db.repository::<ArtifactVersion>();
    match repo.read(&id).await {
        Ok(Some(_)) => {
            return error(
                StatusCode::CONFLICT,
                "this program version has already been published for this platform",
            );
        }
        Ok(None) => {}
        Err(e) => return error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }

    let (Some(staging_path), Some(final_path)) = (
        artifact_staging_path(&program, platform, version_code, &filename),
        artifact_file_path(&program, platform, version_code, &filename),
    ) else {
        return error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid program or filename",
        );
    };

    if let Some(parent) = staging_path.parent()
        && tokio::fs::create_dir_all(parent).await.is_err()
    {
        return error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "could not create the storage directory",
        );
    }

    let (size, sha256) =
        match stream_to_disk(body, &staging_path, crate::routers::files::max_file_bytes()).await {
            Ok(result) => result,
            Err(response) => {
                let _ = tokio::fs::remove_file(&staging_path).await;
                return response;
            }
        };

    // Identity: proven from the archive for android, taken from the URL
    // otherwise.
    let (version_name, label, metadata, arch) = if platform == Platform::Android {
        let meta = match parse_manifest(staging_path.clone()).await {
            Ok(meta) => meta,
            Err(response) => {
                let _ = tokio::fs::remove_file(&staging_path).await;
                return response;
            }
        };
        if meta.package_name != program || meta.version_code != version_code {
            let _ = tokio::fs::remove_file(&staging_path).await;
            return error(
                StatusCode::UNPROCESSABLE_ENTITY,
                &format!(
                    "manifest declares `{}` version {}, which does not match the published path `{program}/android/{version_code}`",
                    meta.package_name, meta.version_code
                ),
            );
        }
        (
            meta.version_name,
            meta.label,
            ArtifactMetadata {
                min_sdk_version: meta.min_sdk_version,
                target_sdk_version: meta.target_sdk_version,
                permissions: meta.permissions,
            },
            None,
        )
    } else {
        (
            params
                .version_name
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| version_code.to_string()),
            params
                .label
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            ArtifactMetadata::default(),
            params
                .arch
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
        )
    };

    if let Some(parent) = final_path.parent()
        && tokio::fs::create_dir_all(parent).await.is_err()
    {
        let _ = tokio::fs::remove_file(&staging_path).await;
        return error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "could not create the storage directory",
        );
    }

    if tokio::fs::rename(&staging_path, &final_path).await.is_err() {
        let _ = tokio::fs::remove_file(&staging_path).await;
        return error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "could not store the artifact",
        );
    }

    let version = ArtifactVersion {
        id,
        program,
        platform: platform.as_str().to_string(),
        arch,
        format,
        version_code,
        version_name,
        filename,
        size_bytes: size as i64,
        sha256,
        label,
        metadata: Json(metadata),
        uploaded_by: actor(&request),
        yanked: false,
        created_at: Utc::now(),
    };

    if let Err(e) = repo.create(&version).await {
        let _ = tokio::fs::remove_file(&final_path).await;
        return error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string());
    }

    HttpResponse::Created().json(ArtifactView::from(&version))
}

/// A conservative charset for the stored `format` tag - it becomes part of a
/// derived filename, so keep it to what a file extension can safely hold.
fn is_format_token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 32
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_')
}

/// Streams `body` into `staging`, returning its size and hex SHA-256.
///
/// A local sibling of `routers::files::ops::upload`'s own streaming loop
/// rather than a shared helper - the two registries don't otherwise share
/// code, and duplicating ~15 lines beats coupling artifact publishing to the
/// files module's internals.
async fn stream_to_disk(
    body: &mut web::Payload,
    staging: &std::path::Path,
    limit: u64,
) -> Result<(u64, String), HttpResponse> {
    let mut file = tokio::fs::File::create(staging).await.map_err(|_| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "could not create the staging file",
        )
    })?;

    let mut size: u64 = 0;
    let mut hasher = Sha256::new();

    while let Some(chunk) = body.next().await {
        let chunk =
            chunk.map_err(|_| error(StatusCode::BAD_REQUEST, "the upload was interrupted"))?;

        size = size.saturating_add(chunk.len() as u64);
        if size > limit {
            return Err(error(
                StatusCode::PAYLOAD_TOO_LARGE,
                &format!("artifact exceeds the {limit}-byte limit"),
            ));
        }

        hasher.update(&chunk);
        file.write_all(&chunk).await.map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "could not write the artifact",
            )
        })?;
    }

    file.flush().await.map_err(|_| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "could not write the artifact",
        )
    })?;

    Ok((size, hex::encode(hasher.finalize())))
}

/// Decodes the manifest on a blocking thread - `zip`'s central-directory scan
/// and `axmldecoder`'s parse are both synchronous, and an APK's central
/// directory sits at the *end* of the file.
///
/// Returns the parse error rather than an `HttpResponse` - `HttpResponse`
/// isn't `Send`, so it can't cross the `spawn_blocking` boundary; the caller
/// turns this into a response once back on the async side.
async fn parse_manifest(
    path: std::path::PathBuf,
) -> Result<apk_manifest::ApkMetadata, HttpResponse> {
    let result = tokio::task::spawn_blocking(move || {
        std::fs::File::open(&path)
            .map_err(|_| ManifestReadError::Reopen)
            .and_then(|file| apk_manifest::extract(file).map_err(ManifestReadError::Manifest))
    })
    .await;

    match result {
        Ok(Ok(metadata)) => Ok(metadata),
        Ok(Err(ManifestReadError::Reopen)) => {
            Err(not_found("could not reopen the uploaded artifact"))
        }
        Ok(Err(ManifestReadError::Manifest(err))) => {
            Err(error(StatusCode::UNPROCESSABLE_ENTITY, &err.to_string()))
        }
        Err(_) => Err(error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "failed to parse the apk manifest",
        )),
    }
}

enum ManifestReadError {
    Reopen,
    Manifest(ApkManifestError),
}
