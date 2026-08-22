//! The realm's token API. This lives in gatehouse rather than in `quench-auth`
//! because only gatehouse serves it: a relying party verifies tokens, it never
//! issues them.

use crate::realm::{self as gh_realm, AuthOutcome};
use actix_web::{HttpRequest, HttpResponse, Responder, cookie::Cookie, get, post, web};
use quench_auth::prelude::realm;
use quench_auth::prelude::{Claims, JwtConfig, Permissions, Session, SessionDb, User, UserDb};
use quench_db::prelude::Db;
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

/// Machine-facing login has no code-entry step of its own, so an
/// `MfaRequired` outcome is reported as a distinct error rather than issuing
/// tokens - a caller that owns a pending token has nowhere to redeem it
/// through this endpoint (that's `ui/pages/auth.rs`'s `/login/mfa`).
#[derive(Serialize)]
struct LoginError {
    error: &'static str,
}

impl LoginError {
    fn response(error: &'static str) -> HttpResponse {
        HttpResponse::Unauthorized().json(LoginError { error })
    }
}

#[post("/login")]
async fn login(
    request: web::Json<LoginRequest>,
    config: web::Data<JwtConfig>,
    db: web::Data<Db>,
    sessions: web::Data<SessionDb>,
) -> impl Responder {
    let outcome = match gh_realm::authenticate(&db, &request.username, &request.password).await {
        Ok(outcome) => outcome,
        Err(err) => {
            tracing::error!("Failed to authenticate {}: {:?}", request.username, err);
            return HttpResponse::InternalServerError().finish();
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
        Ok(tokens) => HttpResponse::Ok().json(tokens),
        Err(err) => {
            tracing::error!("Failed to create authentication session: {}", err);
            HttpResponse::InternalServerError().finish()
        }
    }
}

#[post("/refresh")]
async fn refresh(
    request: HttpRequest,
    body: Option<web::Json<RefreshRequest>>,
    config: web::Data<JwtConfig>,
    users: web::Data<std::sync::Arc<UserDb>>,
    sessions: web::Data<SessionDb>,
) -> impl Responder {
    let cookie_refresh_token = request
        .cookie(&realm::refresh_cookie_name())
        .map(|cookie| cookie.value().to_string());
    let cookie_flow = body.is_none() && cookie_refresh_token.is_some();
    let Some(refresh_token) = body
        .map(|request| request.refresh_token.clone())
        .or(cookie_refresh_token)
    else {
        return HttpResponse::BadRequest().finish();
    };
    let rotated = match sessions
        .rotate(&refresh_token, config.refresh_token_ttl_secs)
        .await
    {
        Ok(Some(rotated)) => rotated,
        Ok(None) => return HttpResponse::Unauthorized().finish(),
        Err(err) => {
            tracing::error!("Failed to rotate refresh token: {}", err);
            return HttpResponse::InternalServerError().finish();
        }
    };
    let (session, refresh_token) = rotated;
    let Some(user) = users.get_user(&session.username).await else {
        return HttpResponse::Unauthorized().finish();
    };
    match token_response(&config, &user, &session, refresh_token).await {
        Ok(tokens) if cookie_flow => {
            let access_cookie = access_cookie(&config, tokens.access_token.clone());
            let refresh_cookie = refresh_cookie(&config, tokens.refresh_token.clone());
            HttpResponse::Ok()
                .cookie(access_cookie)
                .cookie(refresh_cookie)
                .json(tokens)
        }
        Ok(tokens) => HttpResponse::Ok().json(tokens),
        Err(err) => {
            tracing::error!("Failed to issue access token: {}", err);
            HttpResponse::InternalServerError().finish()
        }
    }
}

#[post("/logout")]
async fn logout(
    request: web::Json<RefreshRequest>,
    sessions: web::Data<SessionDb>,
) -> impl Responder {
    match sessions
        .revoke_by_refresh_token(&request.refresh_token)
        .await
    {
        Ok(_) => HttpResponse::NoContent().finish(),
        Err(err) => {
            tracing::error!("Failed to revoke session: {}", err);
            HttpResponse::InternalServerError().finish()
        }
    }
}

#[derive(Serialize)]
pub struct UserInfo {
    pub sub: String,
    pub roles: Vec<String>,
    pub audiences: Vec<String>,
    /// The `service:action` grants the token carries. Empty for a wildcard
    /// role - `admin` in `roles` is what says "everything", and `/api/v1/me`
    /// is the endpoint that resolves that into an answer per service.
    pub permissions: Permissions,
}

/// Subject and roles behind a valid access token. Relying parties use this to
/// resolve an identity without reading the realm's tables themselves.
#[get("/userinfo")]
async fn userinfo(
    request: HttpRequest,
    config: web::Data<JwtConfig>,
    sessions: web::Data<SessionDb>,
) -> impl Responder {
    match access_claims(&request, &config, &sessions).await {
        Some(claims) => HttpResponse::Ok().json(UserInfo {
            // Roles only: the permission entries are reported separately rather
            // than mixed into the role list, which is what splitting the raw
            // scope string used to do.
            roles: claims
                .roles()
                .into_iter()
                .filter(|entry| !entry.contains(':'))
                .collect(),
            permissions: claims.permissions(),
            sub: claims.sub,
            audiences: claims.aud,
        }),
        None => HttpResponse::Unauthorized().finish(),
    }
}

pub fn scope() -> actix_web::Scope {
    web::scope("/api/v1/auth")
        .service(login)
        .service(refresh)
        .service(logout)
        .service(userinfo)
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

/// Same as `issue_token_pair`, but for the `authorization_code` grant
/// (`crate::api::oauth`): the token's audience is narrowed to the requesting
/// client rather than every service the user happens to hold grants on - the
/// whole point of a relying party fetching its own token instead of trusting
/// a realm-wide one.
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

/// The `client_credentials` grant: an access-only token identifying the
/// client itself rather than a user, scoped to the wildcard `service` role -
/// the same access Basic auth used to grant sage against switchboard. No
/// session, so no refresh token either; a client just asks again.
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

/// The scope claim: roles, then one `service:action` entry per granted
/// action. A service granted several actions gets one token per action
/// (`sage:read sage:write`), not a combined one - the wire format is a flat
/// list of space-separated tokens either way, and that keeps the parser on
/// the other end (`Claims::permissions`) simple.
///
/// A wildcard role emits the role alone. Expanding it into a grant per service
/// would make the token bigger, go stale when the estate gained a service, and
/// tell a reader less - `admin` is the more informative claim.
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

/// Services this user's token is valid for.
///
/// The point of narrowing: the audience check already in every relying party's
/// middleware then rejects a user with no grant, so service-level access
/// enforces itself without a single line changing outside gatehouse.
///
/// Gatehouse is always included. It serves the login page, the home page and
/// refresh, so a token that excluded it would leave the user unable to reach the
/// thing that would grant them anything.
pub fn user_audiences(config: &JwtConfig, user: &User) -> Vec<String> {
    if user.has_wildcard() {
        return config.audiences.clone();
    }

    let mut wanted: Vec<String> = user.get_permissions().into_keys().collect();
    wanted.push(config.service_name.clone());

    let mut audiences = config.narrow_audiences(&wanted);
    // `narrow_audiences` filters against SERVICE_AUDIENCES, which need not list
    // gatehouse itself.
    if !audiences.contains(&config.service_name) {
        audiences.push(config.service_name.clone());
    }
    audiences
}

/// Realm-wide session cookie. The `config` argument is no longer read - the
/// name is shared across services now - but is kept so call sites do not churn.
pub fn access_cookie(_config: &JwtConfig, token: String) -> Cookie<'static> {
    realm::session_cookie(token)
}

pub fn refresh_cookie(_config: &JwtConfig, token: String) -> Cookie<'static> {
    realm::refresh_cookie(token)
}

async fn access_claims(
    request: &HttpRequest,
    config: &JwtConfig,
    sessions: &SessionDb,
) -> Option<Claims> {
    let token = request
        .headers()
        .get("Authorization")?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")?;
    let claims = config.decode_claims(token).await.ok()?;
    let session_id = claims.sid.as_deref()?;
    let active = sessions.is_active(session_id, &claims.sub).await.ok()?;
    (claims.allows(&config.service_name) && active).then_some(claims)
}
