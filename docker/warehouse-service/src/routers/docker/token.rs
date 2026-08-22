use crate::docker_token::{DockerClaims, DockerTokenConfig};
use actix_web::{HttpRequest, HttpResponse, Responder, get, http::header, web};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::{Duration, Utc};
use quench_auth::prelude::UserDb;
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
    config: web::Data<DockerTokenConfig>,
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

    // Registry tokens stay single-audience: they are minted for this service's
    // docker endpoint only, never for the realm at large - and never leave
    // warehouse, so they carry no `aud` list at all, just `service`.
    let claims = DockerClaims {
        sub: username,
        service: query.service.clone(),
        scope: query.scope.clone().unwrap_or("docker".to_string()),
        iat: now.timestamp() as usize,
        exp: exp.timestamp() as usize,
    };

    let token = config.encode(&claims).unwrap();

    HttpResponse::Ok().json(TokenResponse {
        token,
        expires_in: 600,
        issued_at: now.to_rfc3339(),
    })
}

/// Basic auth only - the docker registry protocol's own exchange. There is no
/// UI-session fallback: a docker client never carries the realm cookie, and
/// the estate's SSO flow has no say over this endpoint.
async fn validate_basic(
    req: &HttpRequest,
    config: &DockerTokenConfig,
    user_db: &UserDb,
) -> Option<String> {
    if !config.auth_enabled {
        return Some("anonymous".to_string());
    }

    let header_value = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())?;
    let encoded = header_value.strip_prefix("Basic ")?;
    validate_basic_encoded(encoded, user_db).await
}

pub async fn validate_basic_encoded(encoded: &str, user_db: &UserDb) -> Option<String> {
    let decoded = STANDARD.decode(encoded).ok()?;
    let creds = String::from_utf8(decoded).ok()?;
    let (username, password) = creds.split_once(':')?;

    if user_db.validate(username, password).await.is_some() {
        Some(username.to_string())
    } else {
        None
    }
}
