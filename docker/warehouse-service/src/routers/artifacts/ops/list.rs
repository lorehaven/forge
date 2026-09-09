//! `GET /api/v1/artifacts/{program}/{platform}` - one program's versions on
//! one platform, newest first.
//! `GET /api/v1/artifacts/{program}` - every platform+version for one program.
//! `GET /api/v1/artifacts?platform=<tag>` - the catalog: the latest offerable
//! version per `(program, platform)`, optionally filtered to one platform.
//!
//! The catalog is what a client's index page hits - one row per program (per
//! platform), so it doesn't have to fetch every version just to find the
//! newest one it can install.

use crate::domain::artifact::{ArtifactVersion, Platform};
use crate::routers::artifacts::ops::{ArtifactView, disabled, error, find_program_platform};
use crate::routers::artifacts::validate_program;
use actix_web::http::StatusCode;
use actix_web::{HttpResponse, Responder, get, web};
use quench_db::prelude::{Crud, Db};
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Default, Deserialize)]
pub struct CatalogQuery {
    /// Restrict the catalog to one platform tag. An unknown tag yields an
    /// empty catalog rather than an error.
    pub platform: Option<String>,
}

#[get("/{program}/{platform}")]
#[tracing::instrument]
pub async fn platform_versions(
    db: web::Data<Db>,
    path: web::Path<(String, String)>,
) -> impl Responder {
    if !crate::routers::artifacts_enabled() {
        return disabled();
    }

    let (program, platform_raw) = path.into_inner();
    let Some(platform) = Platform::parse(&platform_raw) else {
        return HttpResponse::Ok().json(Vec::<ArtifactView>::new());
    };
    if !validate_program(&program) {
        return HttpResponse::Ok().json(Vec::<ArtifactView>::new());
    }

    let mut versions = match find_program_platform(&db, &program, platform).await {
        Ok(versions) => versions,
        Err(response) => return response,
    };
    versions.sort_by_key(|v| std::cmp::Reverse(v.version_code));

    HttpResponse::Ok().json(versions.iter().map(ArtifactView::from).collect::<Vec<_>>())
}

#[get("/{program}")]
#[tracing::instrument]
pub async fn program_versions(db: web::Data<Db>, path: web::Path<String>) -> impl Responder {
    if !crate::routers::artifacts_enabled() {
        return disabled();
    }

    let program = path.into_inner();
    if !validate_program(&program) {
        return HttpResponse::Ok().json(Vec::<ArtifactView>::new());
    }

    let mut versions = match db
        .repository::<ArtifactVersion>()
        .find_by("program", &program)
        .await
    {
        Ok(versions) => versions,
        Err(e) => return error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };
    // Grouped by platform, newest first within each.
    versions.sort_by(|a, b| {
        a.platform
            .cmp(&b.platform)
            .then(b.version_code.cmp(&a.version_code))
    });

    HttpResponse::Ok().json(versions.iter().map(ArtifactView::from).collect::<Vec<_>>())
}

#[get("")]
#[tracing::instrument]
pub async fn catalog(db: web::Data<Db>, query: web::Query<CatalogQuery>) -> impl Responder {
    if !crate::routers::artifacts_enabled() {
        return disabled();
    }

    // An explicit but unknown `?platform=` tag -> empty catalog, the same
    // non-answer an unauthorised caller gets.
    let platform_filter = match query.platform.as_deref() {
        Some(tag) => match Platform::parse(tag) {
            Some(p) => Some(p),
            None => return HttpResponse::Ok().json(Vec::<ArtifactView>::new()),
        },
        None => None,
    };

    let all = match db.repository::<ArtifactVersion>().list().await {
        Ok(all) => all,
        Err(e) => return error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };

    // (program, platform) -> its rows.
    let mut by_key: HashMap<(&str, &str), Vec<&ArtifactVersion>> = HashMap::new();
    for version in &all {
        if let Some(p) = platform_filter
            && version.platform != p.as_str()
        {
            continue;
        }
        by_key
            .entry((version.program.as_str(), version.platform.as_str()))
            .or_default()
            .push(version);
    }

    let mut catalog: Vec<ArtifactView> = by_key
        .values()
        .filter_map(|rows| {
            rows.iter()
                .filter(|v| !v.yanked)
                .max_by_key(|v| v.version_code)
        })
        .map(|version| ArtifactView::from(*version))
        .collect();
    catalog.sort_by(|a, b| a.program.cmp(&b.program).then(a.platform.cmp(&b.platform)));

    HttpResponse::Ok().json(catalog)
}
