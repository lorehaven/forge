//! `/api/v1/apk/*` - the pre-multi-platform routes, kept working while Pedlar moves to
//! `/api/v1/artifacts`. Thin `platform=android` wrappers over [`super::ops`]; remove once unused.

use crate::domain::artifact::{ArtifactVersion, Platform};
use crate::routers::artifacts::ops::publish::{PublishParams, publish as publish_inner};
use crate::routers::artifacts::ops::yank::set_yanked;
use crate::routers::artifacts::ops::{
    Actor, ArtifactView, disabled, error, find_program_platform, json_ok, latest, not_found,
};
use crate::routers::artifacts::validate_program;
use crate::routers::docker::RawBody;
use quench_db::prelude::{Crud, Db};
use quench_http::prelude::{Inject, Path, Query, Response, delete, get, http::StatusCode, put};

const ANDROID: &str = "android";

#[put("/api/v1/apk/{package}/{version_code}")]
#[tracing::instrument(skip(body))]
pub async fn publish(
    Actor(actor): Actor,
    Inject(db): Inject<Db>,
    Path((program, version_code)): Path<(String, i64)>,
    Query(params): Query<PublishParams>,
    body: RawBody,
) -> Response {
    publish_inner(
        actor,
        &db,
        program,
        Platform::Android,
        version_code,
        params,
        body.0,
    )
    .await
}

#[get("/api/v1/apk/{package}/{version_code}")]
#[tracing::instrument]
pub async fn metadata(
    Inject(db): Inject<Db>,
    Path((program, version_code_raw)): Path<(String, String)>,
) -> Response {
    if !crate::routers::artifacts_enabled() {
        return disabled();
    }

    if version_code_raw == "latest" {
        return match latest::resolve_latest(&db, &program, ANDROID).await {
            Ok(Some(version)) => json_ok(&ArtifactView::from(&version)),
            Ok(None) => not_found("package has no offerable version"),
            Err(response) => response,
        };
    }

    let Ok(version_code) = version_code_raw.parse::<i64>() else {
        return not_found("package or version not found");
    };
    let id = ArtifactVersion::id_for(&program, Platform::Android, version_code);
    match db.repository::<ArtifactVersion>().read(&id).await {
        Ok(Some(version)) => json_ok(&ArtifactView::from(&version)),
        _ => not_found("package or version not found"),
    }
}

#[get("/api/v1/apk/{package}/{version_code}/download")]
#[tracing::instrument]
pub async fn download(
    Inject(db): Inject<Db>,
    Path((program, version_code_raw)): Path<(String, String)>,
) -> Response {
    if !crate::routers::artifacts_enabled() {
        return disabled();
    }

    if version_code_raw == "latest" {
        return match latest::resolve_latest(&db, &program, ANDROID).await {
            Ok(Some(version)) => super::ops::download::serve(&version).await,
            Ok(None) => not_found("package has no offerable version"),
            Err(response) => response,
        };
    }

    let Ok(version_code) = version_code_raw.parse::<i64>() else {
        return not_found("package or version not found");
    };
    let id = ArtifactVersion::id_for(&program, Platform::Android, version_code);
    match db.repository::<ArtifactVersion>().read(&id).await {
        Ok(Some(version)) => super::ops::download::serve(&version).await,
        _ => not_found("package or version not found"),
    }
}

#[get("/api/v1/apk/{package}")]
#[tracing::instrument]
pub async fn versions(Inject(db): Inject<Db>, Path(program): Path<String>) -> Response {
    if !crate::routers::artifacts_enabled() {
        return disabled();
    }
    if !validate_program(&program) {
        return json_ok(&Vec::<ArtifactView>::new());
    }
    let mut versions = match find_program_platform(&db, &program, Platform::Android).await {
        Ok(v) => v,
        Err(response) => return response,
    };
    versions.sort_by_key(|v| std::cmp::Reverse(v.version_code));
    json_ok(&versions.iter().map(ArtifactView::from).collect::<Vec<_>>())
}

#[get("/api/v1/apk")]
#[tracing::instrument]
pub async fn catalog(Inject(db): Inject<Db>) -> Response {
    if !crate::routers::artifacts_enabled() {
        return disabled();
    }
    let all = match db.repository::<ArtifactVersion>().list().await {
        Ok(all) => all,
        Err(e) => return error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };

    use std::collections::HashMap;
    let mut by_program: HashMap<&str, Vec<&ArtifactVersion>> = HashMap::new();
    for version in &all {
        if version.platform != ANDROID {
            continue;
        }
        by_program
            .entry(version.program.as_str())
            .or_default()
            .push(version);
    }
    let mut catalog: Vec<ArtifactView> = by_program
        .values()
        .filter_map(|rows| {
            rows.iter()
                .filter(|v| !v.yanked)
                .max_by_key(|v| v.version_code)
        })
        .map(|v| ArtifactView::from(*v))
        .collect();
    catalog.sort_by(|a, b| a.program.cmp(&b.program));
    json_ok(&catalog)
}

#[delete("/api/v1/apk/{package}/{version_code}/yank")]
#[tracing::instrument]
pub async fn yank(
    Inject(db): Inject<Db>,
    Path((program, version_code)): Path<(String, i64)>,
) -> Response {
    if !crate::routers::artifacts_enabled() {
        return disabled();
    }
    set_yanked(&db, &program, ANDROID, version_code, true).await
}

#[put("/api/v1/apk/{package}/{version_code}/unyank")]
#[tracing::instrument]
pub async fn unyank(
    Inject(db): Inject<Db>,
    Path((program, version_code)): Path<(String, i64)>,
) -> Response {
    if !crate::routers::artifacts_enabled() {
        return disabled();
    }
    set_yanked(&db, &program, ANDROID, version_code, false).await
}

pub fn register_routes() {
    let _ = publish as fn(_, _, _, _, _) -> _;
    let _ = metadata as fn(_, _) -> _;
    let _ = download as fn(_, _) -> _;
    let _ = versions as fn(_, _) -> _;
    let _ = catalog as fn(_) -> _;
    let _ = yank as fn(_, _) -> _;
    let _ = unyank as fn(_, _) -> _;
}
