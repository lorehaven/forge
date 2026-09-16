//! Login/logout belong to gatehouse; this service only hands the browser over.

use quench_auth::domain::jwt::JwtConfig;
use quench_auth::http::domain::sso_client::SsoConfig;
use quench_auth::http::routers::ui::pages::auth::{
    auth_callback, auth_status, login_delegation, logout_delegation, refresh_delegation,
};
use quench_http::prelude::{FromRequest, HttpError, Request, Response, get, post};

pub(super) struct LoginRedirect(Response);

#[async_trait::async_trait]
impl FromRequest for LoginRedirect {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
        let sso = req.container().get::<SsoConfig>().map_err(|e| {
            HttpError::status(http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        })?;
        Ok(LoginRedirect(login_delegation(req, &sso).await))
    }
}

#[get("/ui/login")]
pub(super) async fn login(LoginRedirect(resp): LoginRedirect) -> Response {
    resp
}

#[get("/ui/login/")]
pub(super) async fn login_slash(LoginRedirect(resp): LoginRedirect) -> Response {
    resp
}

pub(super) struct AuthCallback(Response);

#[async_trait::async_trait]
impl FromRequest for AuthCallback {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
        let sso = req.container().get::<SsoConfig>().map_err(|e| {
            HttpError::status(http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        })?;
        Ok(AuthCallback(auth_callback(req, &sso).await))
    }
}

#[get("/ui/auth/callback")]
pub(super) async fn callback(AuthCallback(resp): AuthCallback) -> Response {
    resp
}

pub(super) struct Logout(Response);

#[async_trait::async_trait]
impl FromRequest for Logout {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
        Ok(Logout(logout_delegation(req)))
    }
}

#[get("/ui/logout")]
pub(super) async fn logout(Logout(resp): Logout) -> Response {
    resp
}

pub(super) struct AuthStatusResponse(Response);

#[async_trait::async_trait]
impl FromRequest for AuthStatusResponse {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
        let config = req.container().get::<JwtConfig>().map_err(|e| {
            HttpError::status(http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        })?;
        Ok(AuthStatusResponse(auth_status(req, &config).await))
    }
}

/// What the page shell's session watcher polls.
#[get("/ui/status")]
pub(super) async fn status(AuthStatusResponse(resp): AuthStatusResponse) -> Response {
    resp
}

pub(super) struct Refresh(Response);

#[async_trait::async_trait]
impl FromRequest for Refresh {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
        Ok(Refresh(refresh_delegation(req).await))
    }
}

#[post("/ui/refresh")]
pub(super) async fn refresh(Refresh(resp): Refresh) -> Response {
    resp
}

pub(super) fn register_routes() {
    let _ = login as fn(_) -> _;
    let _ = login_slash as fn(_) -> _;
    let _ = callback as fn(_) -> _;
    let _ = logout as fn(_) -> _;
    let _ = status as fn(_) -> _;
    let _ = refresh as fn(_) -> _;
}
