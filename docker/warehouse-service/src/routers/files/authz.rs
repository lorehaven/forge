//! Resource-scoped access for dynamic storages: private by default (owner,
//! explicit grant, or wildcard role). Static storages keep the blanket check.

use crate::domain::storage::DynamicStorage;
use quench_auth::domain::jwt::{Claims, JwtConfig};

/// Owner, wildcard role, or an explicit `warehouse:storage:<name>:<action>` grant.
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

pub fn can_on_storage_claims(claims: &Claims, storage: &DynamicStorage, action: &str) -> bool {
    claims.has_wildcard()
        || storage.owner == claims.sub
        || claims.can("warehouse", &format!("storage:{}:{}", storage.name, action))
}

/// Blanket `warehouse:<action>` grant - storage *administration* and static-storage writes.
pub fn has_blanket(claims: Option<&Claims>, config: &JwtConfig, action: &str) -> bool {
    if !config.auth_enabled {
        return true;
    }

    claims.is_some_and(|claims| claims.can("warehouse", action))
}
