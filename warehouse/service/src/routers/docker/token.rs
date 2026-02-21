use crate::shared::jwt::{Claims, JwtConfig};
use actix_web::{HttpRequest, HttpResponse, Responder, get, web};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::{Duration, Utc};
use jsonwebtoken::{EncodingKey, Header, encode};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Deserialize, ToSchema)]
pub struct TokenQuery {
    pub service: String,
    pub scope: Option<String>,
    pub account: Option<String>,
    pub client_id: Option<String>,
    pub offline_token: Option<bool>,
}

#[derive(Serialize, ToSchema)]
pub struct TokenResponse {
    pub token: String,
    pub expires_in: usize,
    pub issued_at: String,
}

#[utoipa::path(
    get,
    path = "",
    operation_id = "get_token",
    tags = ["docker"],
    params(
        ("service" = String, Query, description = "Registry service name"),
        ("scope" = String, Query, description = "Requested repository scope")
    ),
    responses(
        (status = 200, description = "JWT token issued", body = TokenResponse),
        (status = 400, description = "Invalid service"),
        (status = 401, description = "Authentication required")
    )
)]
#[get("/token")]
pub async fn handle(
    req: HttpRequest,
    config: web::Data<JwtConfig>,
    query: web::Query<TokenQuery>,
) -> impl Responder {
    // Validate Basic authentication (or allow anonymous if disabled)
    let username = match validate_basic(&req, &config) {
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

fn validate_basic(req: &HttpRequest, config: &JwtConfig) -> Option<String> {
    if !config.auth_enabled {
        return Some("anonymous".to_string());
    }

    let header = req
        .headers()
        .get("Authorization")
        .and_then(|h| h.to_str().ok())?;

    let encoded = header.strip_prefix("Basic ")?;

    let decoded = STANDARD.decode(encoded).ok()?;
    let credentials = String::from_utf8(decoded).ok()?;
    let (username, password) = credentials.split_once(':')?;

    if config.username.as_deref() == Some(username) && config.password.as_deref() == Some(password)
    {
        Some(username.to_string())
    } else {
        None
    }
}
