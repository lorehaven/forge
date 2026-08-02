//! Resource-scoped access, on top of the blanket `conveyor:write`/`conveyor:read`
//! grant `Auth`/`Claims::can` already understand.
//!
//! A grant naming a project directly - `conveyor:project:<id>:<action>` -
//! covers that project and everything nested beneath it, because the check
//! below walks from the target up to the root and accepts a match anywhere
//! along the way. This is the one place that walk happens: gatehouse never
//! learns conveyor's tree shape, and quench-auth's `Claims::can` needed no
//! change at all - the scope wire format already tolerates the extra colons,
//! it is only conveyor that knows what they mean.
//!
//! Kept out of `RequireWrite`. That middleware's own doc comment already
//! covers this exact case: a service whose writes need something more
//! specific than the generic `"write"` has no business behind it - guard the
//! route directly instead. Every repo/project-scoped route in this service
//! does that here rather than relying on the blanket middleware.

use crate::scheduler::projects;
use actix_web::{HttpMessage, HttpRequest, web};
use quench_auth::prelude::{Claims, JwtConfig};
use quench_db::prelude::Db;

/// Whether the realm-wide dev switch is off. Mirrors the bypass every other
/// auth-adjacent check in this estate makes (`Auth`, `RequireWrite`): with
/// auth disabled there is no verified identity to check anything against, so
/// the safe behaviour is the same "let it through" every other check already
/// gives writes and reads alike, not a 403 that only this one check would
/// produce.
fn auth_disabled(request: &HttpRequest) -> bool {
    !request
        .app_data::<web::Data<JwtConfig>>()
        .is_some_and(|config| config.auth_enabled)
}

/// Whether the caller behind `request` may perform `action` ("read" or
/// "write") on `project_id`, through either the unscoped `conveyor:<action>`
/// grant or a resource-scoped grant on `project_id` or one of its ancestors.
pub async fn can_on_project(request: &HttpRequest, db: &Db, project_id: &str, action: &str) -> bool {
    if auth_disabled(request) {
        return true;
    }

    let Some(claims) = request.extensions().get::<Claims>().cloned() else {
        return false;
    };

    can_on_project_claims(&claims, db, project_id, action).await
}

pub async fn can_on_project_claims(claims: &Claims, db: &Db, project_id: &str, action: &str) -> bool {
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

/// Whether the caller behind `request` holds the blanket `conveyor:<action>`
/// grant - used for a write with no project to scope it to yet, like
/// registering a root-level project.
pub fn can_unscoped(request: &HttpRequest, action: &str) -> bool {
    if auth_disabled(request) {
        return true;
    }

    request
        .extensions()
        .get::<Claims>()
        .is_some_and(|claims| claims.can("conveyor", action))
}

/// The project ids `claims` is directly (not through inheritance) granted
/// `action` on via a resource-scoped `conveyor:project:<id>:<action>` entry.
/// The read side of the walk `can_on_project` does from the other direction.
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
