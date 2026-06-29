use actix_web::{HttpRequest, HttpResponse, Responder, get, http::header, web};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::{Duration, Utc};
use jsonwebtoken::{EncodingKey, Header, encode};
use quench_auth::prelude::UserDb;
use quench_auth::prelude::{Claims, JwtConfig};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct TokenQuery {
    pub service: String,
    pub scope: Option<String>,
    pub account: Option<String>,
    pub client_id: Option<String>,
    pub offline_token: Option<bool>,
}

#[derive(Serialize)]
pub struct TokenResponse {
    pub token: String,
    pub expires_in: usize,
    pub issued_at: String,
}

#[get("/token")]
pub async fn handle(
    req: HttpRequest,
    config: web::Data<JwtConfig>,
    user_db: web::Data<std::sync::Arc<UserDb>>,
    query: web::Query<TokenQuery>,
) -> impl Responder {
    // Validate Basic authentication (or allow anonymous if disabled)
    let username = match validate_basic(&req, &config, &user_db).await {
        Some(u) => u,
        None => {
            return HttpResponse::Unauthorized()
                .append_header(("WWW-Authenticate", "Basic realm=\"registry\""))
                .finish();
        }
    };

    // Validate service
    if query.service != config.service_name {
        return HttpResponse::BadRequest().finish();
    }

    let now = Utc::now();
    let exp = now + Duration::minutes(10);

    let claims = Claims {
        sub: username,
        service: query.service.clone(),
        scope: query.scope.clone().unwrap_or("docker".to_string()),
        iat: now.timestamp() as usize,
        exp: exp.timestamp() as usize,
        sid: None,
    };

    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(&config.jwt_secret),
    )
    .unwrap();

    HttpResponse::Ok().json(TokenResponse {
        token,
        expires_in: 600,
        issued_at: now.to_rfc3339(),
    })
}

async fn validate_basic(req: &HttpRequest, config: &JwtConfig, user_db: &UserDb) -> Option<String> {
    if !config.auth_enabled {
        return Some("anonymous".to_string());
    }

    // 1. Try Authorization header
    if let Some(header_value) = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        && let Some(encoded) = header_value.strip_prefix("Basic ")
        && let Some(username) = validate_basic_encoded(encoded, user_db).await
    {
        return Some(username);
    }

    // 2. Fallback to HttpOnly cookie (UI session)
    let cookie_name = format!("{}_ui_session", config.service_name);
    if let Some(cookie) = req.cookie(&cookie_name)
        && let Ok(claims) = config.decode_claims(cookie.value())
    {
        return Some(claims.sub);
    }

    None
}

async fn validate_basic_encoded(encoded: &str, user_db: &UserDb) -> Option<String> {
    let decoded = STANDARD.decode(encoded).ok()?;
    let creds = String::from_utf8(decoded).ok()?;
    let (username, password) = creds.split_once(':')?;

    if user_db.validate(username, password).await.is_some() {
        Some(username.to_string())
    } else {
        None
    }
}
