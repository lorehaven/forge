//! `/api/v1/apk/*` - the pre-multi-platform routes, kept working while the
//! deployed Pedlar moves to `/api/v1/artifacts`. Every handler here is the
//! `/artifacts` equivalent with `platform` pinned to `android`; the actual
//! work lives in [`super::ops`]. Slated for removal once no shipped client
//! calls `/api/v1/apk` (see the module docs in [`super`]).

use crate::domain::artifact::{ArtifactVersion, Platform};
use crate::routers::artifacts::ops::publish::{PublishParams, publish as publish_inner};
use crate::routers::artifacts::ops::yank::set_yanked;
use crate::routers::artifacts::ops::{
    ArtifactView, disabled, error, find_program_platform, not_found,
};
use crate::routers::artifacts::validate_program;
use actix_web::http::StatusCode;
use actix_web::{HttpRequest, HttpResponse, Responder, delete, get, put, web};
use quench_db::prelude::{Crud, Db};

const ANDROID: &str = "android";

#[put("/{package}/{version_code}")]
#[tracing::instrument(skip(body, request))]
pub async fn publish(
    request: HttpRequest,
    db: web::Data<Db>,
    path: web::Path<(String, i64)>,
    params: web::Query<PublishParams>,
    mut body: web::Payload,
) -> impl Responder {
    let (program, version_code) = path.into_inner();
    publish_inner(
        request,
        db.get_ref(),
        program,
        Platform::Android,
        version_code,
        params.into_inner(),
        &mut body,
    )
    .await
}

#[get("/{package}/latest")]
#[tracing::instrument]
pub async fn latest_metadata(db: web::Data<Db>, path: web::Path<String>) -> impl Responder {
    if !crate::routers::artifacts_enabled() {
        return disabled();
    }
    match super::ops::latest::resolve_latest(&db, &path.into_inner(), ANDROID).await {
        Ok(Some(version)) => HttpResponse::Ok().json(ArtifactView::from(&version)),
        Ok(None) => not_found("package has no offerable version"),
        Err(response) => response,
    }
}

#[get("/{package}/latest/download")]
#[tracing::instrument]
pub async fn latest_download(db: web::Data<Db>, path: web::Path<String>) -> impl Responder {
    if !crate::routers::artifacts_enabled() {
        return disabled();
    }
    match super::ops::latest::resolve_latest(&db, &path.into_inner(), ANDROID).await {
        Ok(Some(version)) => super::ops::download::serve(&version).await,
        Ok(None) => not_found("package has no offerable version"),
        Err(response) => response,
    }
}

#[get("/{package}/{version_code}/download")]
#[tracing::instrument]
pub async fn download(db: web::Data<Db>, path: web::Path<(String, i64)>) -> impl Responder {
    if !crate::routers::artifacts_enabled() {
        return disabled();
    }
    let (program, version_code) = path.into_inner();
    let id = ArtifactVersion::id_for(&program, Platform::Android, version_code);
    match db.repository::<ArtifactVersion>().read(&id).await {
        Ok(Some(version)) => super::ops::download::serve(&version).await,
        _ => not_found("package or version not found"),
    }
}

#[get("/{package}/{version_code}")]
#[tracing::instrument]
pub async fn metadata(db: web::Data<Db>, path: web::Path<(String, i64)>) -> impl Responder {
    if !crate::routers::artifacts_enabled() {
        return disabled();
    }
    let (program, version_code) = path.into_inner();
    let id = ArtifactVersion::id_for(&program, Platform::Android, version_code);
    match db.repository::<ArtifactVersion>().read(&id).await {
        Ok(Some(version)) => HttpResponse::Ok().json(ArtifactView::from(&version)),
        _ => not_found("package or version not found"),
    }
}

#[get("/{package}")]
#[tracing::instrument]
pub async fn versions(db: web::Data<Db>, path: web::Path<String>) -> impl Responder {
    if !crate::routers::artifacts_enabled() {
        return disabled();
    }
    let program = path.into_inner();
    if !validate_program(&program) {
        return HttpResponse::Ok().json(Vec::<ArtifactView>::new());
    }
    let mut versions = match find_program_platform(&db, &program, Platform::Android).await {
        Ok(v) => v,
        Err(response) => return response,
    };
    versions.sort_by_key(|v| std::cmp::Reverse(v.version_code));
    HttpResponse::Ok().json(versions.iter().map(ArtifactView::from).collect::<Vec<_>>())
}

#[get("")]
#[tracing::instrument]
pub async fn catalog(db: web::Data<Db>) -> impl Responder {
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
    HttpResponse::Ok().json(catalog)
}

#[delete("/{package}/{version_code}/yank")]
#[tracing::instrument]
pub async fn yank(db: web::Data<Db>, path: web::Path<(String, i64)>) -> impl Responder {
    if !crate::routers::artifacts_enabled() {
        return disabled();
    }
    let (program, version_code) = path.into_inner();
    set_yanked(&db, &program, ANDROID, version_code, true).await
}

#[put("/{package}/{version_code}/unyank")]
#[tracing::instrument]
pub async fn unyank(db: web::Data<Db>, path: web::Path<(String, i64)>) -> impl Responder {
    if !crate::routers::artifacts_enabled() {
        return disabled();
    }
    let (program, version_code) = path.into_inner();
    set_yanked(&db, &program, ANDROID, version_code, false).await
}
