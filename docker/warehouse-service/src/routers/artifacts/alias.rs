//! `/api/v1/apk/*` - the pre-multi-platform routes, kept working while Pedlar moves to
//! `/api/v1/artifacts`. Thin `platform=android` wrappers over [`super::ops`]; remove once unused.
//!
//! Deprecated: [`Deprecation`] logs every call (who, what, which client) and marks the response
//! with `Deprecation`/`Sunset`/`Link` headers; `FEATURE_APK_ALIAS_ENABLED=false` turns the whole
//! prefix into `410 Gone`, so the removal is a config change once the log goes quiet.

use crate::domain::artifact::{ArtifactVersion, Platform};
use crate::routers::artifacts::ops::publish::{PublishParams, publish as publish_inner};
use crate::routers::artifacts::ops::yank::set_yanked;
use crate::routers::artifacts::ops::{
    Actor, ArtifactView, disabled, error, find_program_platform, json_ok, latest, not_found,
};
use crate::routers::artifacts::validate_program;
use crate::routers::docker::RawBody;
use quench_auth::domain::jwt::Claims;
use quench_core::async_trait;
use quench_db::prelude::{Crud, Db};
use quench_http::prelude::{
    Endpoint, Inject, Middleware, Path, Query, Request, Response, delete, get, http::StatusCode,
    put,
};
use std::collections::HashMap;

const ANDROID: &str = "android";

/// Whether the deprecated `/api/v1/apk` routes still answer. Read fresh every call so a test can
/// flip it; defaults to on.
#[must_use]
pub fn alias_enabled() -> bool {
    crate::routers::feature_enabled("FEATURE_APK_ALIAS_ENABLED", true)
}

/// The `Sunset` header value, from `APK_ALIAS_SUNSET` (an HTTP-date, e.g.
/// `Wed, 31 Dec 2026 00:00:00 GMT`); `None` while no removal date is scheduled.
#[must_use]
pub fn alias_sunset() -> Option<String> {
    let value = quench_core::env::get_or("APK_ALIAS_SUNSET", "");
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

/// Usage logging and deprecation signalling for `/api/v1/apk`. Mounted inside `Auth` so the log
/// can name the caller; an unauthenticated probe never reaches it.
#[derive(Debug)]
pub struct Deprecation {
    successor: String,
}

impl Deprecation {
    /// `successor` is the path prefix callers should move to, e.g. `/warehouse/api/v1/artifacts`.
    pub fn new(successor: impl Into<String>) -> Self {
        Self {
            successor: successor.into(),
        }
    }
}

#[async_trait]
impl Middleware for Deprecation {
    async fn handle(&self, req: Request, next: &dyn Endpoint) -> Response {
        let subject = req
            .extensions()
            .get::<Claims>()
            .map_or_else(|| "anonymous".to_string(), |claims| claims.sub.clone());
        let user_agent = req.header("user-agent").unwrap_or("-").to_string();
        let method = req.method().clone();
        let path = req.uri().path().to_string();

        if !alias_enabled() {
            quench_log::warn!(
                %method, %path, %subject, %user_agent,
                "deprecated /api/v1/apk call refused: alias is switched off"
            );
            return Response::json(
                StatusCode::GONE,
                &quench_serde::json!({
                    "errors": [{
                        "detail": format!("/api/v1/apk has been removed; use {}", self.successor)
                    }]
                }),
            )
            .unwrap_or_else(|_| Response::new(StatusCode::GONE));
        }

        quench_log::warn!(
            %method, %path, %subject, %user_agent,
            "deprecated /api/v1/apk call - migrate to {}", self.successor
        );

        let mut response = next.call(req).await.header("deprecation", "true").header(
            "link",
            format!("<{}>; rel=\"successor-version\"", self.successor),
        );
        if let Some(sunset) = alias_sunset() {
            response = response.header("sunset", sunset);
        }
        response
    }
}

#[put("/api/v1/apk/{package}/{version_code}")]
#[quench_log::instrument(skip(body))]
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
#[quench_log::instrument]
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
#[quench_log::instrument]
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
#[quench_log::instrument]
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
#[quench_log::instrument]
pub async fn catalog(Inject(db): Inject<Db>) -> Response {
    if !crate::routers::artifacts_enabled() {
        return disabled();
    }
    let all = match db.repository::<ArtifactVersion>().list().await {
        Ok(all) => all,
        Err(e) => return error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };

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
#[quench_log::instrument]
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
#[quench_log::instrument]
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
