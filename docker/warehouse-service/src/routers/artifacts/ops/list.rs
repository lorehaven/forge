//! Per-program version listing, per-program-all-platforms listing, and the catalog (latest
//! offerable version per program+platform) a client's index page hits.

use crate::domain::artifact::{ArtifactVersion, Platform};
use crate::routers::artifacts::ops::{
    ArtifactView, disabled, error, find_program_platform, json_ok,
};
use crate::routers::artifacts::validate_program;
use quench_db::prelude::{Crud, Db};
use quench_http::prelude::{Inject, Path, Query, Response, get, http::StatusCode};
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Default, Deserialize)]
pub struct CatalogQuery {
    /// An unknown tag yields an empty catalog rather than an error.
    pub platform: Option<String>,
}

#[get("/api/v1/artifacts/{program}/{platform}")]
#[tracing::instrument]
pub async fn platform_versions(
    Inject(db): Inject<Db>,
    Path((program, platform_raw)): Path<(String, String)>,
) -> Response {
    if !crate::routers::artifacts_enabled() {
        return disabled();
    }

    let Some(platform) = Platform::parse(&platform_raw) else {
        return json_ok(&Vec::<ArtifactView>::new());
    };
    if !validate_program(&program) {
        return json_ok(&Vec::<ArtifactView>::new());
    }

    let mut versions = match find_program_platform(&db, &program, platform).await {
        Ok(versions) => versions,
        Err(response) => return response,
    };
    versions.sort_by_key(|v| std::cmp::Reverse(v.version_code));

    json_ok(&versions.iter().map(ArtifactView::from).collect::<Vec<_>>())
}

#[get("/api/v1/artifacts/{program}")]
#[tracing::instrument]
pub async fn program_versions(Inject(db): Inject<Db>, Path(program): Path<String>) -> Response {
    if !crate::routers::artifacts_enabled() {
        return disabled();
    }

    if !validate_program(&program) {
        return json_ok(&Vec::<ArtifactView>::new());
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

    json_ok(&versions.iter().map(ArtifactView::from).collect::<Vec<_>>())
}

#[get("/api/v1/artifacts")]
#[tracing::instrument]
pub async fn catalog(Inject(db): Inject<Db>, Query(query): Query<CatalogQuery>) -> Response {
    if !crate::routers::artifacts_enabled() {
        return disabled();
    }

    // An unknown `?platform=` tag is an empty catalog, not an error.
    let platform_filter = match query.platform.as_deref() {
        Some(tag) => match Platform::parse(tag) {
            Some(p) => Some(p),
            None => return json_ok(&Vec::<ArtifactView>::new()),
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

    json_ok(&catalog)
}

pub fn register_routes() {
    let _ = platform_versions as fn(_, _) -> _;
    let _ = program_versions as fn(_, _) -> _;
    let _ = catalog as fn(_, _) -> _;
}
