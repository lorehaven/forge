//! Resource-scoped access for dynamic storages: private by default (owner,
//! explicit grant, or wildcard role). Static storages keep the blanket check.

use crate::domain::storage::DynamicStorage;
use quench_auth::domain::jwt::{Claims, JwtConfig};

/// Owner, wildcard role, or an explicit `warehouse:storage:<name>:<action>` grant.
#[must_use]
pub fn can_on_storage(
    claims: Option<&Claims>,
    config: &JwtConfig,
    storage: &DynamicStorage,
    action: &str,
) -> bool {
    if !config.auth_enabled {
        return true;
    }

    let Some(claims) = claims else {
        return false;
    };

    can_on_storage_claims(claims, storage, action)
}

#[must_use]
pub fn can_on_storage_claims(claims: &Claims, storage: &DynamicStorage, action: &str) -> bool {
    claims.has_wildcard()
        || storage.owner == claims.sub
        || claims.can("warehouse", &format!("storage:{}:{}", storage.name, action))
}

/// A fixed storage (from `FILE_STORAGES`, no owner): a wildcard role, the blanket grant, or an explicit
/// `warehouse:storage:<name>:<action>` grant. Whoever may write may also read.
///
/// Unlike a dynamic storage, which is private to its owner, a fixed one is shared by default, so the blanket grant applies:
/// `warehouse:read` reads every fixed storage and `warehouse:write` writes every one. The per-storage grant narrows that
/// for someone who has neither.
#[must_use]
pub fn can_on_static(
    claims: Option<&Claims>,
    config: &JwtConfig,
    storage: &str,
    action: &str,
) -> bool {
    if !config.auth_enabled {
        return true;
    }

    let Some(claims) = claims else {
        return false;
    };

    can_on_static_claims(claims, storage, action)
}

#[must_use]
pub fn can_on_static_claims(claims: &Claims, storage: &str, action: &str) -> bool {
    let holds = |grant: &str| claims.can("warehouse", grant);
    let reads = action == "read";
    holds(action)
        || (reads && holds("write"))
        || holds(&format!("storage:{storage}:{action}"))
        || (reads && holds(&format!("storage:{storage}:write")))
}

/// Blanket `warehouse:<action>` grant - storage *administration* (creating and editing storages).
#[must_use]
pub fn has_blanket(claims: Option<&Claims>, config: &JwtConfig, action: &str) -> bool {
    if !config.auth_enabled {
        return true;
    }

    claims.is_some_and(|claims| claims.can("warehouse", action))
}
