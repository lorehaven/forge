//! `PUT .../{name}/{version}` - publish a package. The archive is fully read and checked
//! (see `rivet_package::Package::read`) and its own `rivet.toml` must agree with the URL, so the
//! catalog can't be made to say something the archive doesn't. Republishing is `409`.

use crate::domain::rivet::RivetPackage;
use crate::routers::docker::RawBody;
use crate::routers::rivets::ops::{Actor, RivetView, disabled, error};
use crate::routers::rivets::{package_file_path, package_staging_path, valid_name, valid_version};
use chrono::Utc;
use futures_util::StreamExt;
use http_body_util::BodyExt;
use quench_db::prelude::{Crud, Db};
use quench_http::prelude::{Inject, Path, Response, http::StatusCode, put};
use rivet_package::{Limits, Package, PackageError};
use sha2::{Digest, Sha256};
use sqlx::types::Json;
use tokio::io::AsyncWriteExt;

#[put("/api/v1/rivets/{name}/{version}")]
#[tracing::instrument(skip(body))]
pub async fn handle(
    Actor(actor): Actor,
    Inject(db): Inject<Db>,
    Path((name, version)): Path<(String, String)>,
    body: RawBody,
) -> Response {
    if !crate::routers::rivets_enabled() {
        return disabled();
    }

    if !valid_name(&name) {
        return error(StatusCode::UNPROCESSABLE_ENTITY, "invalid package name");
    }
    if !valid_version(&version) {
        return error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "version must be semver (`1.2.3`, optionally `+build`)",
        );
    }

    let id = RivetPackage::id_for(&name, &version);
    let repo = db.repository::<RivetPackage>();
    match repo.read(&id).await {
        Ok(Some(_)) => {
            return error(
                StatusCode::CONFLICT,
                "this package version has already been published",
            );
        }
        Ok(None) => {}
        Err(e) => return error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }

    let (Some(staging_path), Some(final_path)) = (
        package_staging_path(&name, &version),
        package_file_path(&name, &version),
    ) else {
        return error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid package name or version",
        );
    };

    let Some(parent) = final_path.parent() else {
        return error(StatusCode::INTERNAL_SERVER_ERROR, "invalid storage path");
    };
    if tokio::fs::create_dir_all(parent).await.is_err() {
        return error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "could not create the storage directory",
        );
    }

    let (size, sha256) = match stream_to_disk(body.0, &staging_path, max_bytes()).await {
        Ok(result) => result,
        Err(response) => {
            let _ = tokio::fs::remove_file(&staging_path).await;
            return response;
        }
    };

    let package = match read_package(staging_path.clone()).await {
        Ok(package) => package,
        Err(response) => {
            let _ = tokio::fs::remove_file(&staging_path).await;
            return response;
        }
    };

    if package.manifest.package.name != name || package.manifest.package.version != version {
        let _ = tokio::fs::remove_file(&staging_path).await;
        return error(
            StatusCode::UNPROCESSABLE_ENTITY,
            &format!(
                "the archive declares `{}` version {}, which does not match the published path `{name}/{version}`",
                package.manifest.package.name, package.manifest.package.version
            ),
        );
    }

    // A hard link fails if the target exists, which makes it the no-clobber move `rename` is not:
    // of two concurrent publishes of one version, exactly one gets here.
    let linked = tokio::fs::hard_link(&staging_path, &final_path).await;
    let _ = tokio::fs::remove_file(&staging_path).await;
    match linked {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            return error(
                StatusCode::CONFLICT,
                "this package version has already been published",
            );
        }
        Err(_) => {
            return error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "could not store the package",
            );
        }
    }

    let manifest = match serde_json::to_value(&package.manifest) {
        Ok(value) => value,
        Err(e) => {
            let _ = tokio::fs::remove_file(&final_path).await;
            return error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string());
        }
    };

    let row = RivetPackage {
        id,
        name,
        filename: rivet_package::file_name(&package.manifest.package.name, &version),
        version,
        description: package.manifest.package.description.clone(),
        namespace: package.manifest.package.namespace.clone(),
        size_bytes: i64::try_from(size).unwrap_or(i64::MAX),
        sha256,
        manifest: Json(manifest),
        uploaded_by: actor,
        yanked: false,
        created_at: Utc::now(),
    };

    if let Err(e) = repo.create(&row).await {
        let _ = tokio::fs::remove_file(&final_path).await;
        return error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string());
    }

    Response::json(StatusCode::CREATED, &RivetView::from(&row))
        .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
}

/// Largest archive accepted, compressed. Read fresh so it is testable.
fn max_bytes() -> u64 {
    envmnt::get_u64("RIVET_MAX_BYTES", 64 * 1024 * 1024)
}

/// Reads and validates the staged archive on a blocking thread - zstd and tar are synchronous.
async fn read_package(path: std::path::PathBuf) -> Result<Package, Response> {
    let result = tokio::task::spawn_blocking(move || {
        let file = std::fs::File::open(&path).map_err(PackageError::Io)?;
        Package::read(file, &Limits::default())
    })
    .await;

    match result {
        Ok(Ok(package)) => Ok(package),
        Ok(Err(PackageError::Io(_))) => Err(error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "could not read the uploaded package",
        )),
        Ok(Err(invalid)) => Err(error(
            StatusCode::UNPROCESSABLE_ENTITY,
            &invalid.to_string(),
        )),
        Err(_) => Err(error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "failed to validate the package",
        )),
    }
}

/// Streams `body` into `staging`, returning its size and hex SHA-256.
/// A local sibling of the artifact publish's loop - not shared, to avoid coupling the two.
async fn stream_to_disk(
    body: quench_http::body::InboundBody,
    staging: &std::path::Path,
    limit: u64,
) -> Result<(u64, String), Response> {
    let mut file = tokio::fs::File::create(staging).await.map_err(|_| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "could not create the staging file",
        )
    })?;

    let mut size: u64 = 0;
    let mut hasher = Sha256::new();

    let mut stream = body.into_data_stream();
    while let Some(chunk) = stream.next().await {
        let chunk =
            chunk.map_err(|_| error(StatusCode::BAD_REQUEST, "the upload was interrupted"))?;

        size = size.saturating_add(chunk.len() as u64);
        if size > limit {
            return Err(error(
                StatusCode::PAYLOAD_TOO_LARGE,
                &format!("package exceeds the {limit}-byte limit"),
            ));
        }

        hasher.update(&chunk);
        file.write_all(&chunk).await.map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "could not write the package",
            )
        })?;
    }

    file.flush().await.map_err(|_| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "could not write the package",
        )
    })?;

    Ok((size, hex::encode(hasher.finalize())))
}

pub fn register_routes() {
    let _ = handle as fn(_, _, _, _) -> _;
}
