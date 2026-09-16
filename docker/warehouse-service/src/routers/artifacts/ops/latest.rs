//! Highest non-yanked `version_code` for `.../latest[/download]`. No routes of its own - `latest`
//! is a sentinel inside `metadata`/`download`'s handlers, not a route (would ambiguously overlap).

use crate::domain::artifact::{ArtifactVersion, Platform};
use crate::routers::artifacts::ops::{find_program_platform, latest_of};
use quench_db::prelude::Db;
use quench_http::prelude::Response;

/// Shared by `metadata::handle`/`download::handle` and the `/api/v1/apk` alias.
pub async fn resolve_latest(
    db: &Db,
    program: &str,
    platform_raw: &str,
) -> Result<Option<ArtifactVersion>, Response> {
    let Some(platform) = Platform::parse(platform_raw) else {
        return Ok(None);
    };
    let versions = find_program_platform(db, program, platform).await?;
    Ok(latest_of(&versions).cloned())
}
