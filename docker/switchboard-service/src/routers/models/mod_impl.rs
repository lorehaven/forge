use async_trait::async_trait;
use quench_auth::domain::jwt::{Claims, JwtConfig};
use quench_auth::domain::realm;
use quench_auth::http::domain::cookies::cookie_value;
use quench_http::prelude::{FromRequest, HttpError, Request};
use std::sync::LazyLock;

pub static HF_ROOTS: LazyLock<Vec<String>> =
    LazyLock::new(|| load_paths("HF_ROOTS", &["/mnt/dev/huggingface/hub"]));

pub static GGUF_ROOTS: LazyLock<Vec<String>> =
    LazyLock::new(|| load_paths("GGUF_ROOTS", &["/mnt/dev/quantized"]));

/// The caller's claims, from `Auth`-populated extensions or a decoded
/// session cookie - quench-http never hands a handler the raw `Request`.
pub struct OptionalClaims(pub Option<Claims>);

#[async_trait]
impl FromRequest for OptionalClaims {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
        if let Some(claims) = req.extensions().get::<Claims>() {
            return Ok(OptionalClaims(Some(claims.clone())));
        }

        let Ok(config) = req.container().get::<JwtConfig>() else {
            return Ok(OptionalClaims(None));
        };
        let Some(cookie) = cookie_value(req, &realm::session_cookie_name()) else {
            return Ok(OptionalClaims(None));
        };

        match config.decode_claims(&cookie).await {
            Ok(claims) => Ok(OptionalClaims(Some(claims))),
            Err(_) => Ok(OptionalClaims(None)),
        }
    }
}

/// Whether the caller holds a wildcard role (admin/service account).
/// `/models/running` stays admin-only via this rather than a `read` grant.
pub fn is_admin(claims: Option<&Claims>, config: &JwtConfig) -> bool {
    !config.auth_enabled || claims.is_some_and(Claims::has_wildcard)
}

/// Whether the caller may perform `action` - replaces the blanket
/// `RequireWrite` these scopes deliberately don't declare a `"write"` for.
pub fn can(claims: Option<&Claims>, config: &JwtConfig, action: &str) -> bool {
    !config.auth_enabled || claims.is_some_and(|c| c.can(&config.service_name, action))
}

pub fn load_paths(env_key: &str, defaults: &[&str]) -> Vec<String> {
    std::env::var(env_key)
        .ok()
        .map(|v| {
            v.split(':')
                .filter(|s| !s.trim().is_empty())
                .map(|s| s.trim().to_string())
                .collect()
        })
        .filter(|v: &Vec<String>| !v.is_empty())
        .unwrap_or_else(|| defaults.iter().map(|s| s.to_string()).collect())
}
