//! What the pages share: who is asking, and where a visitor with no session is sent.

pub use quench_auth::domain::jwt::Claims;
use quench_auth::domain::jwt::JwtConfig;
use quench_auth::http::routers::ui::get_user_from_req;
use quench_core::async_trait;
use quench_http::prelude::{FromRequest, HttpError, Request, Response};
pub use quench_starter::http::routers::ui::{
    is_ui_authenticated, ui_asset_path, ui_login_redirect_for, ui_path,
};

#[must_use]
pub fn ui_login_redirect() -> Response {
    quench_starter::http::routers::ui::ui_login_redirect()
}

/// The signed-in identity from the session cookie. `None` means "not signed in", not "auth disabled" -
/// that folds into a synthetic all-access `Claims`.
pub async fn actor(request: &Request, config: &JwtConfig) -> Option<Claims> {
    get_user_from_req(request, config).await
}

/// Claims, or the redirect to send instead.
pub enum ActorOrRedirect {
    Claims(Claims),
    Redirect(Response),
}

impl ActorOrRedirect {
    pub fn or_redirect(self) -> Result<Claims, Response> {
        match self {
            Self::Claims(claims) => Ok(claims),
            Self::Redirect(resp) => Err(resp),
        }
    }
}

#[async_trait]
impl FromRequest for ActorOrRedirect {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
        let Ok(config) = req.container().get::<JwtConfig>() else {
            return Ok(Self::Redirect(ui_login_redirect_for(req)));
        };
        actor(req, &config).await.map_or_else(
            || Ok(Self::Redirect(ui_login_redirect_for(req))),
            |claims| Ok(Self::Claims(claims)),
        )
    }
}

impl std::fmt::Debug for ActorOrRedirect {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ActorOrRedirect { .. }")
    }
}
