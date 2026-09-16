//! Who may *change* things from the management UI: viewing needs only a
//! session; mutating needs the blanket `warehouse:write` grant or a wildcard role.

use async_trait::async_trait;
use quench_auth::domain::jwt::{Claims, JwtConfig};
use quench_auth::http::routers::ui::get_user_from_req;
use quench_http::prelude::{FromRequest, HttpError, Request, Response, http::StatusCode};
use quench_starter::http::routers::ui::ui_login_redirect_for;

/// Whether `claims` may perform a management mutation in the warehouse UI.
pub fn can_manage(claims: &Claims) -> bool {
    claims.can("warehouse", "write")
}

/// The caller's claims from the realm session cookie, or `None` if there's no usable session.
pub async fn ui_claims(request: &Request, config: &JwtConfig) -> Option<Claims> {
    get_user_from_req(request, config).await
}

/// The caller's claims, for a page that also needs to know if it may manage
/// this content (to show/hide a mutating control) beyond the plain `PageAuth` gate.
pub struct OptionalUiClaims(pub Option<Claims>);

#[async_trait]
impl FromRequest for OptionalUiClaims {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
        let Ok(config) = req.container().get::<JwtConfig>() else {
            return Ok(Self(None));
        };
        Ok(Self(ui_claims(req, &config).await))
    }
}

/// A management-mutation gate. The redirect is resolved during extraction and
/// carried as the success value, since `HttpError` can only render fixed text, never a redirect.
pub enum ManageGate {
    Allowed,
    Forbidden,
    Redirect(Response),
}

impl ManageGate {
    /// `Ok(())` when allowed; otherwise a login redirect (no session) or a 403 (no grant).
    pub fn or_response(self) -> Result<(), Response> {
        match self {
            Self::Allowed => Ok(()),
            Self::Forbidden => Err(Response::text(StatusCode::FORBIDDEN, "api_error_forbidden")),
            Self::Redirect(response) => Err(response),
        }
    }
}

#[async_trait]
impl FromRequest for ManageGate {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
        let Ok(config) = req.container().get::<JwtConfig>() else {
            return Ok(Self::Redirect(ui_login_redirect_for(req)));
        };
        match ui_claims(req, &config).await {
            Some(claims) if can_manage(&claims) => Ok(Self::Allowed),
            Some(_) => Ok(Self::Forbidden),
            None => Ok(Self::Redirect(ui_login_redirect_for(req))),
        }
    }
}
