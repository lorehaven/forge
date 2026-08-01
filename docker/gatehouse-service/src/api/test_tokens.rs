//! Token minting for the BDD harness (`tests/forge-bdd`), enabled only by
//! `GATEHOUSE_TEST_MODE=true`.
//!
//! A scenario asks for a token with an arbitrary subject, audience, scope,
//! and - for testing exp/iat edge cases - explicit timestamps, without a real
//! user or session existing. This is the same shortcut the harness took when
//! every service verified against one shared HS256 secret; now it is routed
//! through gatehouse's real signing key, so JWKS verification at the relying
//! party sees a legitimately-issued token instead of one it has no way to
//! trust. Unreachable whenever the flag is unset, which is every real
//! deployment - `main.rs` never sets `GATEHOUSE_TEST_MODE`.

use actix_web::{HttpResponse, Responder, post, web};
use quench_auth::prelude::{Claims, JwtConfig};
use serde::Deserialize;

#[derive(Deserialize)]
struct TestTokenRequest {
    sub: String,
    #[serde(default)]
    aud: Vec<String>,
    scope: String,
    /// Unix seconds. Defaults let a scenario mint an ordinary valid token;
    /// overrides are what `with expired token` / `with future iat` scenarios
    /// use to get a token real users never legitimately hold.
    #[serde(default)]
    iat: Option<i64>,
    #[serde(default)]
    exp: Option<i64>,
}

#[post("/api/v1/test/token")]
pub async fn mint(
    config: web::Data<JwtConfig>,
    body: web::Json<TestTokenRequest>,
) -> impl Responder {
    if !envmnt::is_or("GATEHOUSE_TEST_MODE", false) {
        return HttpResponse::NotFound().finish();
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
        Ok(access_token) => {
            HttpResponse::Ok().json(serde_json::json!({ "access_token": access_token }))
        }
        Err(err) => {
            tracing::error!("test token mint failed: {err}");
            HttpResponse::InternalServerError().finish()
        }
    }
}
