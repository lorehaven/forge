//! The realm's token API - lives here, not `quench-auth`, since only
//! gatehouse issues tokens; relying parties only verify them.

use crate::realm::{self as gh_realm, AuthOutcome};
use async_trait::async_trait;
use http::StatusCode;
use quench_auth::domain::auth::{Permissions, User, UserDb};
use quench_auth::domain::jwt::{Claims, JwtConfig};
use quench_auth::domain::realm;
use quench_auth::domain::session::{Session, SessionDb};
use quench_db::prelude::Db;
use quench_http::prelude::{FromRequest, HttpError, Inject, Json, Request, Response, get, post};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: String,
    pub expires_in: i64,
}

/// No code-entry step here, so `MfaRequired` is a distinct error, not a token
/// - redeeming it is `ui/pages/auth.rs`'s `/login/mfa`.
#[derive(Serialize)]
struct LoginError {
    error: &'static str,
}

impl LoginError {
    fn response(error: &'static str) -> Response {
        Response::json(StatusCode::UNAUTHORIZED, &LoginError { error })
            .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
    }
}

#[post("/api/v1/auth/login")]
async fn login(
    Json(request): Json<LoginRequest>,
    Inject(config): Inject<JwtConfig>,
    Inject(db): Inject<Db>,
    Inject(sessions): Inject<SessionDb>,
) -> Response {
    let outcome = match gh_realm::authenticate(&db, &request.username, &request.password).await {
        Ok(outcome) => outcome,
        Err(err) => {
            tracing::error!("Failed to authenticate {}: {:?}", request.username, err);
            return Response::new(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };
    let user = match outcome {
        AuthOutcome::Success(user) => user,
        AuthOutcome::MfaRequired { .. } => return LoginError::response("mfa_required"),
        AuthOutcome::Disabled => return LoginError::response("account_disabled"),
        AuthOutcome::Locked => return LoginError::response("account_locked"),
        AuthOutcome::NotFound | AuthOutcome::WrongPassword => {
            return LoginError::response("invalid_credentials");
        }
    };
    match issue_token_pair(&config, &sessions, &user).await {
        Ok(tokens) => json_ok(&tokens),
        Err(err) => {
            tracing::error!("Failed to create authentication session: {}", err);
            Response::new(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Read directly, since `refresh` needs it even with no JSON body at all.
struct RefreshCookie(Option<String>);

#[async_trait]
impl FromRequest for RefreshCookie {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
        Ok(RefreshCookie(
            quench_auth::http::domain::cookies::cookie_value(req, &realm::refresh_cookie_name()),
        ))
    }
}

#[post("/api/v1/auth/refresh")]
async fn refresh(
    quench_http::prelude::Bytes(raw): quench_http::prelude::Bytes,
    RefreshCookie(cookie_refresh_token): RefreshCookie,
    Inject(config): Inject<JwtConfig>,
    Inject(users): Inject<UserDb>,
    Inject(sessions): Inject<SessionDb>,
) -> Response {
    // No body (or unparsable) means "fall back to the cookie", not a 400.
    let body: Option<RefreshRequest> = if raw.is_empty() {
        None
    } else {
        serde_json::from_slice(&raw).ok()
    };
    let cookie_flow = body.is_none() && cookie_refresh_token.is_some();
    let Some(refresh_token) = body
        .map(|request| request.refresh_token)
        .or(cookie_refresh_token)
    else {
        return Response::new(StatusCode::BAD_REQUEST);
    };
    let rotated = match sessions
        .rotate(&refresh_token, config.refresh_token_ttl_secs)
        .await
    {
        Ok(Some(rotated)) => rotated,
        Ok(None) => return Response::new(StatusCode::UNAUTHORIZED),
        Err(err) => {
            tracing::error!("Failed to rotate refresh token: {}", err);
            return Response::new(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };
    let (session, refresh_token) = rotated;
    let Some(user) = users.get_user(&session.username).await else {
        return Response::new(StatusCode::UNAUTHORIZED);
    };
    match token_response(&config, &user, &session, refresh_token).await {
        Ok(tokens) if cookie_flow => json_ok(&tokens)
            .append_header(
                "set-cookie",
                realm::session_cookie(tokens.access_token.clone()).to_string(),
            )
            .append_header(
                "set-cookie",
                realm::refresh_cookie(tokens.refresh_token.clone()).to_string(),
            ),
        Ok(tokens) => json_ok(&tokens),
        Err(err) => {
            tracing::error!("Failed to issue access token: {}", err);
            Response::new(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

#[post("/api/v1/auth/logout")]
async fn logout(
    Json(request): Json<RefreshRequest>,
    Inject(sessions): Inject<SessionDb>,
) -> Response {
    match sessions
        .revoke_by_refresh_token(&request.refresh_token)
        .await
    {
        Ok(_) => Response::new(StatusCode::NO_CONTENT),
        Err(err) => {
            tracing::error!("Failed to revoke session: {}", err);
            Response::new(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

#[derive(Serialize)]
pub struct UserInfo {
    pub sub: String,
    pub roles: Vec<String>,
    pub audiences: Vec<String>,
    /// `service:action` grants; empty for a wildcard role (`admin` says it all).
    pub permissions: Permissions,
}

struct MaybeAccessClaims(Option<Claims>);

/// Separate from `SubjectClaims` - this only ever accepts a bearer header,
/// never the session cookie or dev-mode bypass.
#[async_trait]
impl FromRequest for MaybeAccessClaims {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
        let config = req
            .container()
            .get::<JwtConfig>()
            .map_err(|e| HttpError::status(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        let sessions = req
            .container()
            .get::<SessionDb>()
            .map_err(|e| HttpError::status(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        let Some(token) = super::users::bearer_token(req) else {
            return Ok(MaybeAccessClaims(None));
        };
        let Ok(claims) = config.decode_claims(&token).await else {
            return Ok(MaybeAccessClaims(None));
        };
        let Some(session_id) = claims.sid.clone() else {
            return Ok(MaybeAccessClaims(None));
        };
        let active = sessions
            .is_active(&session_id, &claims.sub)
            .await
            .unwrap_or(false);
        Ok(MaybeAccessClaims(
            (claims.allows(&config.service_name) && active).then_some(claims),
        ))
    }
}

#[get("/api/v1/auth/userinfo")]
async fn userinfo(MaybeAccessClaims(claims): MaybeAccessClaims) -> Response {
    match claims {
        Some(claims) => json_ok(&UserInfo {
            // Roles only - permissions are reported separately, not mixed in.
            roles: claims
                .roles()
                .into_iter()
                .filter(|entry| !entry.contains(':'))
                .collect(),
            permissions: claims.permissions(),
            sub: claims.sub,
            audiences: claims.aud,
        }),
        None => Response::new(StatusCode::UNAUTHORIZED),
    }
}

pub fn register_routes() {
    let _ = login as fn(_, _, _, _) -> _;
    let _ = refresh as fn(_, _, _, _, _) -> _;
    let _ = logout as fn(_, _) -> _;
    let _ = userinfo as fn(_) -> _;
}

pub async fn issue_token_pair(
    config: &JwtConfig,
    sessions: &SessionDb,
    user: &User,
) -> anyhow::Result<TokenResponse> {
    let (session, refresh_token) = sessions
        .create(&user.username, config.refresh_token_ttl_secs)
        .await?;
    Ok(token_response(config, user, &session, refresh_token).await?)
}

/// Like `issue_token_pair`, but for `authorization_code` (`api::oauth`):
/// audience narrowed to the requesting client, not every grant the user holds.
pub(crate) async fn issue_token_pair_for_client(
    config: &JwtConfig,
    sessions: &SessionDb,
    user: &User,
    client_audiences: &[String],
) -> anyhow::Result<TokenResponse> {
    let (session, refresh_token) = sessions
        .create(&user.username, config.refresh_token_ttl_secs)
        .await?;
    let access_token = config
        .issue_access_token_for(
            user.username.clone(),
            config.narrow_audiences(client_audiences),
            user_scope(user),
            Some(session.id.clone()),
        )
        .await?;
    Ok(TokenResponse {
        access_token,
        refresh_token,
        token_type: "Bearer".to_string(),
        expires_in: config.access_token_ttl_secs,
    })
}

/// `client_credentials` grant: access-only token for the client itself,
/// scoped to `service`. No session, so no refresh token either.
pub(crate) async fn issue_client_credentials_token(
    config: &JwtConfig,
    client_id: &str,
    audiences: &[String],
) -> anyhow::Result<TokenResponse> {
    let access_token = config
        .issue_access_token_for(
            client_id.to_string(),
            config.narrow_audiences(audiences),
            "service".to_string(),
            None,
        )
        .await?;
    Ok(TokenResponse {
        access_token,
        refresh_token: String::new(),
        token_type: "Bearer".to_string(),
        expires_in: config.access_token_ttl_secs,
    })
}

pub async fn token_response(
    config: &JwtConfig,
    user: &User,
    session: &Session,
    refresh_token: String,
) -> Result<TokenResponse, jsonwebtoken::errors::Error> {
    let access_token = config
        .issue_access_token_for(
            user.username.clone(),
            user_audiences(config, user),
            user_scope(user),
            Some(session.id.clone()),
        )
        .await?;
    Ok(TokenResponse {
        access_token,
        refresh_token,
        token_type: "Bearer".to_string(),
        expires_in: config.access_token_ttl_secs,
    })
}

/// Scope claim: roles, then `service:action` per grant (wildcard roles emit the role alone).
pub fn user_scope(user: &User) -> String {
    let mut entries: Vec<String> = user
        .get_roles()
        .iter()
        .map(|role| role.as_str().to_string())
        .collect();

    if !user.has_wildcard() {
        for (service, actions) in user.get_permissions() {
            for action in actions {
                entries.push(format!("{service}:{action}"));
            }
        }
    }

    entries.join(" ")
}

/// Audiences this token is valid for; gatehouse itself is always included (serves login/refresh).
pub fn user_audiences(config: &JwtConfig, user: &User) -> Vec<String> {
    if user.has_wildcard() {
        return config.audiences.clone();
    }

    let mut wanted: Vec<String> = user.get_permissions().into_keys().collect();
    wanted.push(config.service_name.clone());

    let mut audiences = config.narrow_audiences(&wanted);
    // SERVICE_AUDIENCES need not list gatehouse itself.
    if !audiences.contains(&config.service_name) {
        audiences.push(config.service_name.clone());
    }
    audiences
}

fn json_ok<T: Serialize>(value: &T) -> Response {
    Response::json(StatusCode::OK, value)
        .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
}
