//! Token minting for the BDD harness, gated on `GATEHOUSE_TEST_MODE=true`
//! (never set by `main.rs`) - signs with gatehouse's real key so JWKS still trusts it.

use quench_auth::domain::jwt::{Claims, JwtConfig};
use quench_http::prelude::{Inject, Json, Response, post};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct TestTokenRequest {
    sub: String,
    #[serde(default)]
    aud: Vec<String>,
    scope: String,
    /// Unix seconds; overrides mint expired/future-iat tokens for edge-case scenarios.
    #[serde(default)]
    iat: Option<i64>,
    #[serde(default)]
    exp: Option<i64>,
}

#[post("/api/v1/test/token")]
pub async fn mint(
    Inject(config): Inject<JwtConfig>,
    Json(body): Json<TestTokenRequest>,
) -> Response {
    if !envmnt::is_or("GATEHOUSE_TEST_MODE", false) {
        return Response::new(http::StatusCode::NOT_FOUND);
    }

    let now = chrono::Utc::now().timestamp();
    let claims = Claims {
        sub: body.sub.clone(),
        aud: body.aud.clone(),
        scope: body.scope.clone(),
        iat: body.iat.unwrap_or(now) as usize,
        exp: body.exp.unwrap_or(now + config.access_token_ttl_secs) as usize,
        sid: None,
    };

    match config.encode_claims(&claims).await {
        Ok(access_token) => Response::json(
            http::StatusCode::OK,
            &serde_json::json!({ "access_token": access_token }),
        )
        .unwrap_or_else(|_| Response::new(http::StatusCode::INTERNAL_SERVER_ERROR)),
        Err(err) => {
            tracing::error!("test token mint failed: {err}");
            Response::new(http::StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub fn register_routes() {
    let _ = mint as fn(_, _) -> _;
}
