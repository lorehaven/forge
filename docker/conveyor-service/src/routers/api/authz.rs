//! Resource-scoped access on top of the blanket `conveyor:write`/`conveyor:read` grant. A grant naming
//! a project (`conveyor:project:<id>:<action>`) covers it and everything nested beneath.

use crate::scheduler::projects;
use quench_auth::domain::jwt::{Claims, JwtConfig};
use quench_db::prelude::Db;

/// Whether `claims` may `action` on `project_id`, via the unscoped grant or a scoped one on an ancestor.
pub async fn can_on_project(
    claims: Option<&Claims>,
    config: &JwtConfig,
    db: &Db,
    project_id: &str,
    action: &str,
) -> bool {
    if !config.auth_enabled {
        return true;
    }

    let Some(claims) = claims else {
        return false;
    };

    can_on_project_claims(claims, db, project_id, action).await
}

pub async fn can_on_project_claims(
    claims: &Claims,
    db: &Db,
    project_id: &str,
    action: &str,
) -> bool {
    if claims.can("conveyor", action) {
        return true;
    }

    let chain = projects::ancestor_chain(db, project_id)
        .await
        .unwrap_or_default();

    chain
        .iter()
        .any(|id| claims.can("conveyor", &format!("project:{id}:{action}")))
}

/// The blanket `conveyor:<action>` grant - for a write with no project to scope to yet.
pub fn can_unscoped(claims: Option<&Claims>, config: &JwtConfig, action: &str) -> bool {
    if !config.auth_enabled {
        return true;
    }

    claims.is_some_and(|claims| claims.can("conveyor", action))
}

/// Project ids `claims` is directly (not by inheritance) granted `action` on.
pub fn granted_project_ids(claims: &Claims, action: &str) -> Vec<String> {
    claims
        .permissions()
        .get("conveyor")
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let rest = entry.strip_prefix("project:")?;
            let (id, held_action) = rest.rsplit_once(':')?;
            (held_action == action).then(|| id.to_string())
        })
        .collect()
}
