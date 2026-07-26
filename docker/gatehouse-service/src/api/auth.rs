//! The realm's token API. This lives in gatehouse rather than in `quench-auth`
//! because only gatehouse serves it: a relying party verifies tokens, it never
//! issues them.

use actix_web::{HttpRequest, HttpResponse, Responder, cookie::Cookie, get, post, web};
use quench_auth::prelude::realm;
use quench_auth::prelude::{Claims, JwtConfig, Session, SessionDb, User, UserDb};
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

#[post("/login")]
async fn login(
    request: web::Json<LoginRequest>,
    config: web::Data<JwtConfig>,
    users: web::Data<std::sync::Arc<UserDb>>,
    sessions: web::Data<SessionDb>,
) -> impl Responder {
    let Some(user) = users.validate(&request.username, &request.password).await else {
        return HttpResponse::Unauthorized().finish();
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
    match token_response(&config, &user, &session, refresh_token) {
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
            sub: claims.sub,
            roles: claims.scope.split(' ').map(str::to_string).collect(),
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
    Ok(token_response(config, user, &session, refresh_token)?)
}

fn token_response(
    config: &JwtConfig,
    user: &User,
    session: &Session,
    refresh_token: String,
) -> Result<TokenResponse, jsonwebtoken::errors::Error> {
    let access_token = config.issue_access_token(
        user.username.clone(),
        user_scope(user),
        Some(session.id.clone()),
    )?;
    Ok(TokenResponse {
        access_token,
        refresh_token,
        token_type: "Bearer".to_string(),
        expires_in: config.access_token_ttl_secs,
    })
}

fn user_scope(user: &User) -> String {
    user.get_roles()
        .iter()
        .map(|role| format!("{:?}", role).to_lowercase())
        .collect::<Vec<_>>()
        .join(" ")
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
    let claims = config.decode_claims(token).ok()?;
    let session_id = claims.sid.as_deref()?;
    let active = sessions.is_active(session_id, &claims.sub).await.ok()?;
    (claims.allows(&config.service_name) && active).then_some(claims)
}
