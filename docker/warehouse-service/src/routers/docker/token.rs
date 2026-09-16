use crate::docker_token::{DockerClaims, DockerTokenConfig};
use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::{Duration, Utc};
use quench_auth::domain::auth::UserDb;
use quench_http::prelude::{
    FromRequest, HttpError, Inject, Query, Request, Response, get, http::StatusCode,
};
use serde::{Deserialize, Serialize};

/// The raw `Authorization` header, read via a local extractor (quench-http has none built in).
pub struct AuthorizationHeader(pub Option<String>);

#[async_trait]
impl FromRequest for AuthorizationHeader {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
        Ok(Self(req.header("authorization").map(str::to_string)))
    }
}

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
    AuthorizationHeader(authorization): AuthorizationHeader,
    Inject(config): Inject<DockerTokenConfig>,
    Inject(user_db): Inject<UserDb>,
    Query(query): Query<TokenQuery>,
) -> Response {
    // Validate Basic authentication (or allow anonymous if disabled)
    let username = match validate_basic(authorization.as_deref(), &config, &user_db).await {
        Some(u) => u,
        None => {
            return Response::new(StatusCode::UNAUTHORIZED)
                .header("www-authenticate", "Basic realm=\"registry\"");
        }
    };

    // Validate service
    if query.service != config.service_name {
        return Response::new(StatusCode::BAD_REQUEST);
    }

    let now = Utc::now();
    let exp = now + Duration::minutes(10);

    // Single-audience: this endpoint only, no realm-wide `aud` list.
    let claims = DockerClaims {
        sub: username,
        service: query.service.clone(),
        scope: query.scope.clone().unwrap_or("docker".to_string()),
        iat: now.timestamp() as usize,
        exp: exp.timestamp() as usize,
    };

    let token = config.encode(&claims).unwrap();

    Response::json(
        StatusCode::OK,
        &TokenResponse {
            token,
            expires_in: 600,
            issued_at: now.to_rfc3339(),
        },
    )
    .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
}

/// Basic auth only - a docker client never carries the realm session cookie.
async fn validate_basic(
    authorization: Option<&str>,
    config: &DockerTokenConfig,
    user_db: &UserDb,
) -> Option<String> {
    if !config.auth_enabled {
        return Some("anonymous".to_string());
    }

    let encoded = authorization?.strip_prefix("Basic ")?;
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

pub fn register_routes() {
    let _ = handle as fn(_, _, _, _) -> _;
}
