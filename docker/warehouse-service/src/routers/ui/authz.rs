//! Who may *change* things from the management UI.
//!
//! Viewing a management page needs only a realm session for this service, the
//! same bar the docker and crates pages already use (`is_ui_authenticated`).
//! Every mutation - provisioning a dynamic storage, editing its quota, deleting
//! a storage or a file, yanking an APK - is held to the same check the JSON
//! APIs enforce: the blanket `warehouse:write` grant, or a wildcard
//! (`admin`/`service`) role. This is exactly `routers::files::authz::has_blanket`
//! and the APK scope's `RequireWrite`, restated here against a UI cookie
//! instead of a bearer header.

use actix_web::{HttpRequest, HttpResponse};
use quench_auth::prelude::{Claims, JwtConfig};

/// Whether `claims` may perform a management mutation in the warehouse UI.
///
/// Pure and role-only: a wildcard role short-circuits inside
/// [`Claims::can`], so this is `has_wildcard() || can("warehouse", "write")`
/// folded into one call.
pub fn can_manage(claims: &Claims) -> bool {
    claims.can("warehouse", "write")
}

/// The caller's claims from the realm session cookie, or `None` when there is
/// no usable session. With auth disabled this hands back a synthetic wildcard
/// identity, matching every other check in the estate.
pub async fn ui_claims(request: &HttpRequest, config: &JwtConfig) -> Option<Claims> {
    quench_auth::actix::routers::ui::get_user_from_req(request, config).await
}

/// `Ok(())` when the caller may mutate, otherwise the response to return
/// instead: a login redirect when there is no session at all, a plain `403`
/// when there is a session but it lacks `warehouse:write`.
pub async fn require_manage(request: &HttpRequest, config: &JwtConfig) -> Result<(), HttpResponse> {
    match ui_claims(request, config).await {
        Some(claims) if can_manage(&claims) => Ok(()),
        Some(_) => Err(HttpResponse::Forbidden().body("api_error_forbidden")),
        None => Err(quench_starter::actix::routers::ui::ui_login_redirect_for(
            request,
        )),
    }
}
