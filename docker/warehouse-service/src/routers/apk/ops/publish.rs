//! `PUT /api/v1/apk/{package}/{version_code}` - publish an APK.
//!
//! The body is the raw `.apk` file, nothing else - there is no separate
//! metadata payload the way `crates`' publish protocol has one, because the
//! metadata that matters (package name, version) is decoded from the archive
//! itself rather than trusted from the caller. The URL's `{package}` and
//! `{version_code}` are what the caller is *asserting*; the manifest is what
//! decides whether that assertion is true.

use crate::domain::apk::ApkVersion;
use crate::domain::apk_manifest::{self, ApkManifestError};
use crate::routers::apk::ops::{actor, disabled, error, not_found};
use crate::routers::apk::{apk_file_path, apk_staging_path, validate_package_name};
use actix_web::http::StatusCode;
use actix_web::{HttpRequest, HttpResponse, Responder, put, web};
use chrono::Utc;
use futures_util::StreamExt;
use quench_db::prelude::{Crud, Db};
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::types::Json;
use tokio::io::AsyncWriteExt;

#[derive(Serialize)]
pub struct Published {
    pub package_name: String,
    pub version_code: i64,
    pub version_name: String,
    pub min_sdk_version: Option<i32>,
    pub target_sdk_version: Option<i32>,
    pub label: Option<String>,
    pub permissions: Vec<String>,
    pub size_bytes: i64,
    pub sha256: String,
}

#[put("/{package}/{version_code}")]
#[tracing::instrument(skip(body, request))]
pub async fn handle(
    request: HttpRequest,
    db: web::Data<Db>,
    path: web::Path<(String, i64)>,
    mut body: web::Payload,
) -> impl Responder {
    if !crate::routers::apk_enabled() {
        return disabled();
    }

    let (package_name, version_code) = path.into_inner();

    if !validate_package_name(&package_name) {
        return error(StatusCode::UNPROCESSABLE_ENTITY, "invalid package name");
    }
    if version_code <= 0 {
        return error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "version_code must be a positive integer",
        );
    }

    let id = ApkVersion::id_for(&package_name, version_code);
    let repo = db.repository::<ApkVersion>();
    match repo.read(&id).await {
        Ok(Some(_)) => {
            return error(
                StatusCode::CONFLICT,
                "this package version has already been published",
            );
        }
        Ok(None) => {}
        Err(e) => {
            return error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string());
        }
    }

    let (Some(staging_path), Some(final_path)) = (
        apk_staging_path(&package_name, version_code),
        apk_file_path(&package_name, version_code),
    ) else {
        return error(StatusCode::UNPROCESSABLE_ENTITY, "invalid package name");
    };

    if let Some(parent) = staging_path.parent()
        && tokio::fs::create_dir_all(parent).await.is_err()
    {
        return error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "could not create the storage directory",
        );
    }

    let (size, sha256) = match stream_to_disk(
        &mut body,
        &staging_path,
        crate::routers::files::max_file_bytes(),
    )
    .await
    {
        Ok(result) => result,
        Err(response) => {
            let _ = tokio::fs::remove_file(&staging_path).await;
            return response;
        }
    };

    let metadata = match parse_manifest(staging_path.clone()).await {
        Ok(metadata) => metadata,
        Err(response) => {
            let _ = tokio::fs::remove_file(&staging_path).await;
            return response;
        }
    };

    if metadata.package_name != package_name || metadata.version_code != version_code {
        let _ = tokio::fs::remove_file(&staging_path).await;
        return error(
            StatusCode::UNPROCESSABLE_ENTITY,
            &format!(
                "manifest declares `{}` version {}, which does not match the published path `{package_name}/{version_code}`",
                metadata.package_name, metadata.version_code
            ),
        );
    }

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
        return error(StatusCode::INTERNAL_SERVER_ERROR, "could not store the apk");
    }

    let version = ApkVersion {
        id,
        package_name: metadata.package_name.clone(),
        version_code: metadata.version_code,
        version_name: metadata.version_name.clone(),
        min_sdk_version: metadata.min_sdk_version,
        target_sdk_version: metadata.target_sdk_version,
        label: metadata.label.clone(),
        permissions: Json(metadata.permissions.clone()),
        size_bytes: size as i64,
        sha256: sha256.clone(),
        uploaded_by: actor(&request),
        yanked: false,
        created_at: Utc::now(),
    };

    if let Err(e) = repo.create(&version).await {
        let _ = tokio::fs::remove_file(&final_path).await;
        return error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string());
    }

    HttpResponse::Created().json(Published {
        package_name: metadata.package_name,
        version_code: metadata.version_code,
        version_name: metadata.version_name,
        min_sdk_version: metadata.min_sdk_version,
        target_sdk_version: metadata.target_sdk_version,
        label: metadata.label,
        permissions: metadata.permissions,
        size_bytes: size as i64,
        sha256,
    })
}

/// Streams `body` into `staging`, returning its size and hex SHA-256.
///
/// A local sibling of `routers::files::ops::upload`'s own streaming loop
/// rather than a shared helper - the two registries don't otherwise share
/// code, and duplicating ~15 lines beats coupling apk publishing to the
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
                &format!("apk exceeds the {limit}-byte limit"),
            ));
        }

        hasher.update(&chunk);
        file.write_all(&chunk)
            .await
            .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "could not write the apk"))?;
    }

    file.flush()
        .await
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "could not write the apk"))?;

    Ok((size, hex::encode(hasher.finalize())))
}

/// Decodes the manifest on a blocking thread - `zip`'s central-directory scan
/// and `axmldecoder`'s parse are both synchronous, and an APK's central
/// directory sits at the *end* of the file, so this necessarily seeks across
/// however much of it has landed on disk rather than touching only a small
/// header.
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
        Ok(Err(ManifestReadError::Reopen)) => Err(not_found("could not reopen the uploaded apk")),
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
