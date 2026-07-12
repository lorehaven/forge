use crate::actix::domain::auth::{User, UserDb};
use crate::actix::domain::jwt::{Claims, JwtConfig};
use crate::actix::domain::session::{Session, SessionDb};
use actix_web::{
    HttpRequest, HttpResponse, Responder,
    cookie::{Cookie, SameSite},
    delete, get, post, web,
};
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

#[derive(Serialize)]
pub struct SessionResponse {
    pub id: String,
    pub username: String,
    pub created_at: String,
    pub expires_at: String,
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
    let cookie_name = format!("{}_refresh_token", config.service_name);
    let cookie_refresh_token = request
        .cookie(&cookie_name)
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

#[get("/sessions")]
async fn list_sessions(
    request: HttpRequest,
    config: web::Data<JwtConfig>,
    sessions: web::Data<SessionDb>,
) -> impl Responder {
    let Some(claims) = access_claims(&request, &config, &sessions).await else {
        return HttpResponse::Unauthorized().finish();
    };
    match sessions.list_active(&claims.sub).await {
        Ok(items) => HttpResponse::Ok().json(
            items
                .into_iter()
                .map(SessionResponse::from)
                .collect::<Vec<_>>(),
        ),
        Err(err) => {
            tracing::error!("Failed to list sessions: {}", err);
            HttpResponse::InternalServerError().finish()
        }
    }
}

#[delete("/sessions/{id}")]
async fn revoke_session(
    request: HttpRequest,
    id: web::Path<String>,
    config: web::Data<JwtConfig>,
    sessions: web::Data<SessionDb>,
) -> impl Responder {
    let Some(claims) = access_claims(&request, &config, &sessions).await else {
        return HttpResponse::Unauthorized().finish();
    };
    match sessions.revoke(&id, &claims.sub).await {
        Ok(true) => HttpResponse::NoContent().finish(),
        Ok(false) => HttpResponse::NotFound().finish(),
        Err(err) => {
            tracing::error!("Failed to revoke session: {}", err);
            HttpResponse::InternalServerError().finish()
        }
    }
}

pub fn scope() -> actix_web::Scope {
    web::scope("/api/v1/auth")
        .service(login)
        .service(refresh)
        .service(logout)
        .service(list_sessions)
        .service(revoke_session)
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

pub fn access_cookie(config: &JwtConfig, token: String) -> Cookie<'static> {
    Cookie::build(format!("{}_ui_session", config.service_name), token)
        .path("/")
        .http_only(true)
        .same_site(SameSite::Strict)
        .secure(true)
        .finish()
}

pub fn refresh_cookie(config: &JwtConfig, token: String) -> Cookie<'static> {
    Cookie::build(format!("{}_refresh_token", config.service_name), token)
        .path("/")
        .http_only(true)
        .same_site(SameSite::Strict)
        .secure(true)
        .finish()
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
    (claims.service == config.service_name && active).then_some(claims)
}

impl From<Session> for SessionResponse {
    fn from(session: Session) -> Self {
        Self {
            id: session.id,
            username: session.username,
            created_at: session.created_at,
            expires_at: session.expires_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actix::domain::auth::Role;
    use actix_web::{
        App,
        cookie::Cookie,
        http::{StatusCode, header::SET_COOKIE},
        test,
    };
    use quench_db::Db;

    fn jwt_config() -> JwtConfig {
        JwtConfig {
            jwt_secret: b"test-secret".to_vec(),
            service_name: "test-service".to_string(),
            realm: "test".to_string(),
            auth_enabled: true,
            access_token_ttl_secs: 900,
            refresh_token_ttl_secs: 604800,
        }
    }

    #[actix_web::test]
    async fn login_creates_session_and_refresh_rotates_token() {
        let db = Db::InMemory(quench_db::InMemoryDb::new());
        let users = UserDb::init(db.clone()).await;
        users
            .add_user(User::new("user".into(), "password".into(), vec![Role::User]).unwrap())
            .await;
        let sessions = SessionDb::init(db);
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(jwt_config()))
                .app_data(web::Data::new(users))
                .app_data(web::Data::from(sessions))
                .service(scope()),
        )
        .await;

        let login_response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/auth/login")
                .set_json(serde_json::json!({
                    "username": "user",
                    "password": "password"
                }))
                .to_request(),
        )
        .await;
        assert_eq!(login_response.status(), StatusCode::OK);
        let first: TokenResponse = test::read_body_json(login_response).await;

        let refresh_response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/auth/refresh")
                .set_json(serde_json::json!({ "refresh_token": first.refresh_token.clone() }))
                .to_request(),
        )
        .await;
        assert_eq!(refresh_response.status(), StatusCode::OK);
        let second: TokenResponse = test::read_body_json(refresh_response).await;

        let reused_response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/auth/refresh")
                .set_json(serde_json::json!({ "refresh_token": first.refresh_token }))
                .to_request(),
        )
        .await;
        assert_eq!(reused_response.status(), StatusCode::UNAUTHORIZED);
        assert_ne!(first.refresh_token, second.refresh_token);

        let cookie_refresh_response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/auth/refresh")
                .cookie(Cookie::new(
                    "test-service_refresh_token",
                    second.refresh_token,
                ))
                .to_request(),
        )
        .await;
        assert_eq!(cookie_refresh_response.status(), StatusCode::OK);
        assert_eq!(
            cookie_refresh_response
                .headers()
                .get_all(SET_COOKIE)
                .count(),
            2
        );
    }
}
