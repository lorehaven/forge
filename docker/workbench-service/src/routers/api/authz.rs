//! Resource-scoped access, on top of the blanket `workbench:write`/`workbench:read`
//! grant `Auth`/`Claims::can` already understands.
//!
//! A grant naming a project directly - `workbench:project:<id>:<action>` -
//! covers exactly that project. Unlike conveyor's `can_on_project` (`docker/conveyor-service/src/routers/api/authz.rs`),
//! there is no ancestor walk here: workbench's projects are flat, so a
//! resource-scoped grant on a project id is checked directly rather than
//! against a chain built by querying the database - which is also why this
//! module needs no `Db` at all, unlike conveyor's.

use actix_web::{HttpMessage, HttpRequest, web};
use quench_auth::prelude::{Claims, JwtConfig};

/// Whether the realm-wide dev switch is off. Mirrors the bypass every other
/// auth-adjacent check in this estate makes.
fn auth_disabled(request: &HttpRequest) -> bool {
    !request
        .app_data::<web::Data<JwtConfig>>()
        .is_some_and(|config| config.auth_enabled)
}

/// Whether the caller behind `request` may perform `action` ("read" or
/// "write") on `project_id`, through either the unscoped `workbench:<action>`
/// grant or a resource-scoped grant on `project_id` itself.
pub fn can_on_project(request: &HttpRequest, project_id: &str, action: &str) -> bool {
    if auth_disabled(request) {
        return true;
    }

    let Some(claims) = request.extensions().get::<Claims>().cloned() else {
        return false;
    };

    can_on_project_claims(&claims, project_id, action)
}

pub fn can_on_project_claims(claims: &Claims, project_id: &str, action: &str) -> bool {
    claims.can("workbench", action)
        || claims.can("workbench", &format!("project:{project_id}:{action}"))
}

/// Whether the caller behind `request` holds the blanket `workbench:<action>`
/// grant - used for a write with no project to scope it to yet, like creating
/// a new project.
pub fn can_unscoped(request: &HttpRequest, action: &str) -> bool {
    if auth_disabled(request) {
        return true;
    }

    request
        .extensions()
        .get::<Claims>()
        .is_some_and(|claims| claims.can("workbench", action))
}

/// The project ids `claims` is directly granted `action` on via a
/// resource-scoped `workbench:project:<id>:<action>` entry.
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
