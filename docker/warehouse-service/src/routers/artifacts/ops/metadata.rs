//! `GET .../{version_code}` and `.../latest` in one handler - see `super::latest`.

use crate::domain::artifact::{ArtifactVersion, Platform};
use crate::routers::artifacts::ops::{ArtifactView, disabled, json_ok, latest, not_found};
use quench_db::prelude::{Crud, Db};
use quench_http::prelude::{Inject, Path, Response, get};

#[get("/api/v1/artifacts/{program}/{platform}/{version_code}")]
#[tracing::instrument]
pub async fn handle(
    Inject(db): Inject<Db>,
    Path((program, platform_raw, version_code_raw)): Path<(String, String, String)>,
) -> Response {
    if !crate::routers::artifacts_enabled() {
        return disabled();
    }

    if version_code_raw == "latest" {
        return match latest::resolve_latest(&db, &program, &platform_raw).await {
            Ok(Some(version)) => json_ok(&ArtifactView::from(&version)),
            Ok(None) => not_found("program has no offerable version for this platform"),
            Err(response) => response,
        };
    }

    let Ok(version_code) = version_code_raw.parse::<i64>() else {
        return not_found("program or version not found");
    };
    let Some(platform) = Platform::parse(&platform_raw) else {
        return not_found("program or version not found");
    };
    let id = ArtifactVersion::id_for(&program, platform, version_code);

    match db.repository::<ArtifactVersion>().read(&id).await {
        Ok(Some(version)) => json_ok(&ArtifactView::from(&version)),
        _ => not_found("program or version not found"),
    }
}

pub fn register_routes() {
    let _ = handle as fn(_, _) -> _;
}
