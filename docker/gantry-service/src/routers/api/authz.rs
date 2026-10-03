//! What the realm's token says the caller may do here.
//!
//! Actions are `read`, `deploy`, `scale`, `activate` and `rollback` (see Gatehouse's
//! permission catalog). A grant is either blanket (`gantry:deploy`) or scoped to one package
//! (`gantry:target:media:deploy`), the way Conveyor and Warehouse scope theirs: someone can be allowed to
//! upgrade `media` and nothing else.

use quench_auth::domain::jwt::{Claims, JwtConfig};

/// The service's name in the permission catalog.
pub const SERVICE: &str = "gantry";

/// Whether the caller may do `action` anywhere. `auth_enabled` off is the realm-wide dev bypass.
pub fn can(claims: Option<&Claims>, config: &JwtConfig, action: &str) -> bool {
    if !config.auth_enabled {
        return true;
    }
    claims.is_some_and(|claims| claims.can(SERVICE, action))
}

/// Whether the caller may do `action` on the package `target`, by the blanket grant or one scoped to it.
pub fn can_on_target(
    claims: Option<&Claims>,
    config: &JwtConfig,
    target: &str,
    action: &str,
) -> bool {
    if !config.auth_enabled {
        return true;
    }
    claims.is_some_and(|claims| {
        claims.can(SERVICE, action) || claims.can(SERVICE, &format!("target:{target}:{action}"))
    })
}

/// Packages the caller is directly granted `action` on.
pub fn granted_targets(claims: &Claims, action: &str) -> Vec<String> {
    claims
        .permissions()
        .get(SERVICE)
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let rest = entry.strip_prefix("target:")?;
            let (target, held) = rest.rsplit_once(':')?;
            (held == action).then(|| target.to_string())
        })
        .collect()
}

/// Whether the caller may confirm `plan`: the permission its action needs, on every package it touches.
/// Going to an older version needs `rollback`, a swap needs `activate`, a start or stop `scale`, and
/// anything else `deploy`.
pub fn can_confirm(
    claims: Option<&Claims>,
    config: &JwtConfig,
    plan: &crate::domain::operation::StoredPlan,
) -> Result<(), String> {
    let needed = crate::domain::planner::Action::parse(&plan.action)
        .map_or("deploy", crate::domain::planner::Action::permission);
    let touched: Vec<&str> = if plan.plan.touches.is_empty() {
        vec![plan.target.as_str()]
    } else {
        plan.plan.touches.iter().map(String::as_str).collect()
    };
    for target in touched {
        if !can_on_target(claims, config, target, needed) {
            return Err(format!("no {needed} access to {target}"));
        }
    }
    Ok(())
}
