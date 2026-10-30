//! The artifact registry: its published builds, and yanking them.

use super::{Session, ok};
use crate::domain::artifact::ArtifactVersion;
use crate::routers::artifacts::ops::yank::set_yanked;
use quench_db::prelude::{Crud, Db};
use quench_http::prelude::{Inject, Json, Response, get, http::StatusCode, post};
use warehouse_core::{ArtifactEntry, ArtifactRef};

#[must_use]
pub fn entry_of(version: ArtifactVersion) -> ArtifactEntry {
    ArtifactEntry {
        program: version.program,
        platform: version.platform,
        format: version.format,
        version_code: version.version_code,
        version_name: version.version_name,
        filename: version.filename,
        size_bytes: version.size_bytes,
        sha256: version.sha256,
        uploaded_by: version.uploaded_by,
        yanked: version.yanked,
        arch: version.arch,
        label: version.label,
        min_sdk_version: version.metadata.min_sdk_version,
        target_sdk_version: version.metadata.target_sdk_version,
        permissions: version.metadata.permissions,
    }
}

/// Every build. A disabled registry or an unreachable database is an empty list, as for the old page.
#[get("/ui/api/artifacts")]
pub async fn list(session: Session, Inject(db): Inject<Db>) -> Response {
    if let Err(response) = session.require() {
        return response;
    }
    let versions = if crate::routers::artifacts_enabled() {
        db.repository::<ArtifactVersion>()
            .list()
            .await
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    ok(&versions.into_iter().map(entry_of).collect::<Vec<_>>())
}

async fn yank_state(session: Session, db: &Db, body: ArtifactRef, yanked: bool) -> Response {
    if let Err(response) = session.manager() {
        return response;
    }
    if !crate::routers::artifacts_enabled() {
        return super::error(StatusCode::NOT_FOUND, "api_error_artifacts_disabled");
    }
    set_yanked(
        db,
        &body.program,
        &body.platform,
        body.version_code,
        None,
        yanked,
    )
    .await
}

#[post("/ui/api/artifacts/yank")]
pub async fn yank(
    session: Session,
    Inject(db): Inject<Db>,
    Json(body): Json<ArtifactRef>,
) -> Response {
    yank_state(session, &db, body, true).await
}

#[post("/ui/api/artifacts/unyank")]
pub async fn unyank(
    session: Session,
    Inject(db): Inject<Db>,
    Json(body): Json<ArtifactRef>,
) -> Response {
    yank_state(session, &db, body, false).await
}

pub fn register_routes() {
    let _ = list as fn(_, _) -> _;
    let _ = yank as fn(_, _, _) -> _;
    let _ = unyank as fn(_, _, _) -> _;
}
