//! Who may *change* things from the management UI: viewing needs only a session; mutating needs the blanket
//! `warehouse:write` grant or a wildcard role.

use quench_auth::domain::jwt::{Claims, JwtConfig};
use quench_auth::http::routers::ui::get_user_from_req;
use quench_http::prelude::Request;

/// Whether `claims` may perform a management mutation in the warehouse UI.
#[must_use]
pub fn can_manage(claims: &Claims) -> bool {
    claims.can("warehouse", "write")
}

/// The caller's claims from the realm session cookie, or `None` if there's no usable session.
pub async fn ui_claims(request: &Request, config: &JwtConfig) -> Option<Claims> {
    get_user_from_req(request, config).await
}
