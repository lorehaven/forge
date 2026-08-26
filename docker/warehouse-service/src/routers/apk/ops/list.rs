//! `GET /api/v1/apk/{package}` - one package's versions.
//! `GET /api/v1/apk` - the catalog: every package's latest offerable version.
//!
//! The second is what the eventual Android app-store's index page hits - one
//! row per package rather than one per version, so it doesn't have to fetch
//! every version just to find the newest.

use crate::domain::apk::ApkVersion;
use crate::routers::apk::ops::{VersionView, disabled, error};
use crate::routers::apk::validate_package_name;
use actix_web::http::StatusCode;
use actix_web::{HttpResponse, Responder, get, web};
use quench_db::prelude::{Crud, Db};
use std::collections::HashMap;

#[get("/{package}")]
#[tracing::instrument]
pub async fn versions(db: web::Data<Db>, path: web::Path<String>) -> impl Responder {
    if !crate::routers::apk_enabled() {
        return disabled();
    }

    let package_name = path.into_inner();
    if !validate_package_name(&package_name) {
        return HttpResponse::Ok().json(Vec::<VersionView>::new());
    }

    let mut versions = match db
        .repository::<ApkVersion>()
        .find_by("package_name", &package_name)
        .await
    {
        Ok(versions) => versions,
        Err(e) => return error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };
    versions.sort_by_key(|version| std::cmp::Reverse(version.version_code));

    HttpResponse::Ok().json(versions.iter().map(VersionView::from).collect::<Vec<_>>())
}

#[get("")]
#[tracing::instrument]
pub async fn catalog(db: web::Data<Db>) -> impl Responder {
    if !crate::routers::apk_enabled() {
        return disabled();
    }

    let all = match db.repository::<ApkVersion>().list().await {
        Ok(all) => all,
        Err(e) => return error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };

    let mut by_package: HashMap<&str, Vec<&ApkVersion>> = HashMap::new();
    for version in &all {
        by_package
            .entry(version.package_name.as_str())
            .or_default()
            .push(version);
    }

    let mut catalog: Vec<VersionView> = by_package
        .values()
        .filter_map(|package_versions| {
            package_versions
                .iter()
                .filter(|version| !version.yanked)
                .max_by_key(|version| version.version_code)
        })
        .map(|version| VersionView::from(*version))
        .collect();
    catalog.sort_by(|a, b| a.package_name.cmp(&b.package_name));

    HttpResponse::Ok().json(catalog)
}
