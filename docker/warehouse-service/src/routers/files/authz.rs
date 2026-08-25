//! Resource-scoped access for dynamic storages.
//!
//! `conveyor` and `workbench` already have a resource-scoped grant pattern
//! (`docker/conveyor-service/src/routers/api/authz.rs`,
//! `docker/workbench-service/src/routers/api/authz.rs`): a grant naming one
//! project directly is *additive* on top of the blanket
//! `<service>:<action>` grant, because those services want broad access to be
//! the norm.
//!
//! A dynamic storage wants the opposite default - private unless shared -
//! since the whole point is per-user isolation (a phone backup, say). So
//! [`can_on_storage`] deliberately does **not** fall back to the blanket
//! `warehouse:<action>` grant: only the storage's owner, an explicit
//! `warehouse:storage:<name>:<action>` grant, or a wildcard role (admin or
//! service) may touch it. Static, env-configured storages are untouched by
//! any of this - they keep using the blanket check exactly as `RequireWrite`
//! did before this module existed.
//!
//! Kept out of `RequireWrite` for the same reason conveyor/workbench's checks
//! are: that middleware's own doc comment already covers this case - a write
//! that needs something more specific than the generic `"write"` has no
//! business behind it. Every dynamic-storage route guards itself with this
//! module instead.

use crate::domain::storage::DynamicStorage;
use actix_web::{HttpMessage, HttpRequest, web};
use quench_auth::prelude::{Claims, JwtConfig};

/// Whether the realm-wide dev switch is off. Mirrors the bypass every other
/// auth-adjacent check in this estate makes: with auth disabled there is no
/// verified identity to check anything against, so the safe behaviour is the
/// same "let it through" every other check already gives writes and reads
/// alike, not a 403 that only this one check would produce.
fn auth_disabled(request: &HttpRequest) -> bool {
    !request
        .app_data::<web::Data<JwtConfig>>()
        .is_some_and(|config| config.auth_enabled)
}

/// Whether the caller behind `request` may perform `action` ("read" or
/// "write") on `storage` - as its owner, through a wildcard role, or through
/// an explicit `warehouse:storage:<name>:<action>` grant.
pub fn can_on_storage(request: &HttpRequest, storage: &DynamicStorage, action: &str) -> bool {
    if auth_disabled(request) {
        return true;
    }

    let Some(claims) = request.extensions().get::<Claims>().cloned() else {
        return false;
    };

    can_on_storage_claims(&claims, storage, action)
}

pub fn can_on_storage_claims(claims: &Claims, storage: &DynamicStorage, action: &str) -> bool {
    claims.has_wildcard()
        || storage.owner == claims.sub
        || claims.can("warehouse", &format!("storage:{}:{}", storage.name, action))
}

/// Whether the caller behind `request` holds the blanket `warehouse:<action>`
/// grant. Two callers: dynamic-storage *administration* (create/patch/delete),
/// deliberately admin-only rather than owner-or-scoped, since provisioning a
/// storage and assigning its owner is not something the eventual owner does
/// for themselves; and a static storage's write check, which this reproduces
/// exactly as `RequireWrite` used to enforce it.
pub fn has_blanket(request: &HttpRequest, action: &str) -> bool {
    if auth_disabled(request) {
        return true;
    }

    request
        .extensions()
        .get::<Claims>()
        .is_some_and(|claims| claims.can("warehouse", action))
}
