//! Resource-scoped access on top of the blanket `workbench:<action>` grant.
//! Projects are flat, so a scoped grant checks directly - no `Db`, no walk.

use quench_auth::domain::jwt::{Claims, JwtConfig};

/// Whether the caller may do `action` on `project_id`, via the unscoped or
/// resource-scoped grant. `auth_enabled` off is the realm-wide dev bypass.
pub fn can_on_project(
    claims: Option<&Claims>,
    config: &JwtConfig,
    project_id: &str,
    action: &str,
) -> bool {
    if !config.auth_enabled {
        return true;
    }

    let Some(claims) = claims else {
        return false;
    };

    can_on_project_claims(claims, project_id, action)
}

pub fn can_on_project_claims(claims: &Claims, project_id: &str, action: &str) -> bool {
    claims.can("workbench", action)
        || claims.can("workbench", &format!("project:{project_id}:{action}"))
}

/// Whether the caller holds the blanket grant, for writes with no project
/// to scope to yet (e.g. creating one).
pub fn can_unscoped(claims: Option<&Claims>, config: &JwtConfig, action: &str) -> bool {
    if !config.auth_enabled {
        return true;
    }

    claims.is_some_and(|claims| claims.can("workbench", action))
}

/// Project ids `claims` is directly granted `action` on.
pub fn granted_project_ids(claims: &Claims, action: &str) -> Vec<String> {
    claims
        .permissions()
        .get("workbench")
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let rest = entry.strip_prefix("project:")?;
            let (id, held_action) = rest.rsplit_once(':')?;
            (held_action == action).then(|| id.to_string())
        })
        .collect()
}
