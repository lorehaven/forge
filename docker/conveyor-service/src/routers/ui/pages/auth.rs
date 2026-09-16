//! Login/logout belong to gatehouse - this service only hands the browser over.

use quench_auth::domain::jwt::JwtConfig;
use quench_auth::http::domain::sso_client::SsoConfig;
use quench_auth::http::routers::ui::pages::auth::{
    auth_callback, login_delegation, logout_delegation, refresh_delegation,
};
use quench_http::prelude::{FromRequest, HttpError, Inject, Request, Response, get, post};
use serde::Serialize;

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

#[derive(Serialize)]
struct AuthStatus {
    authenticated: bool,
    username: Option<String>,
    roles: Vec<String>,
}

impl AuthStatus {
    fn anonymous() -> Self {
        Self {
            authenticated: false,
            username: None,
            roles: vec![],
        }
    }
}

pub(super) struct SessionCookie(Option<String>);

#[async_trait::async_trait]
impl FromRequest for SessionCookie {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
        Ok(SessionCookie(
            quench_auth::http::domain::cookies::cookie_value(
                req,
                &quench_auth::domain::realm::session_cookie_name(),
            ),
        ))
    }
}

#[get("/ui/status")]
pub(super) async fn auth_status(
    Inject(config): Inject<JwtConfig>,
    SessionCookie(cookie): SessionCookie,
) -> Response {
    fn respond(status: AuthStatus) -> Response {
        Response::json(http::StatusCode::OK, &status)
            .unwrap_or_else(|_| Response::new(http::StatusCode::INTERNAL_SERVER_ERROR))
    }

    if !config.auth_enabled {
        return respond(AuthStatus {
            authenticated: true,
            username: Some("dev".to_string()),
            roles: vec!["admin".to_string()],
        });
    }

    let Some(cookie) = cookie else {
        return respond(AuthStatus::anonymous());
    };

    match config.decode_claims(&cookie).await {
        Ok(claims) => respond(AuthStatus {
            authenticated: true,
            username: Some(claims.sub),
            roles: claims
                .scope
                .split(',')
                .filter(|role| !role.is_empty())
                .map(str::to_string)
                .collect(),
        }),
        Err(_) => respond(AuthStatus::anonymous()),
    }
}

pub(super) fn register_routes() {
    let _ = login as fn(_) -> _;
    let _ = login_slash as fn(_) -> _;
    let _ = callback as fn(_) -> _;
    let _ = logout as fn(_) -> _;
    let _ = refresh as fn(_) -> _;
    let _ = auth_status as fn(_, _) -> _;
}
