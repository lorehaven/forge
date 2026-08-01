use actix_web::web;
use quench_auth::prelude::JwtConfig;
use std::sync::LazyLock;

pub static HF_ROOTS: LazyLock<Vec<String>> =
    LazyLock::new(|| load_paths("HF_ROOTS", &["/mnt/dev/huggingface/hub"]));

pub static GGUF_ROOTS: LazyLock<Vec<String>> =
    LazyLock::new(|| load_paths("GGUF_ROOTS", &["/mnt/dev/quantized"]));

/// Whether the caller holds a wildcard role (admin, or the machine-to-machine
/// `service` account).
///
/// Still used for `/models/running`: which models are currently loaded is
/// operational GPU state, and this endpoint has always treated that as
/// admin-only rather than something a `switchboard:read` grant reaches. The
/// per-action catalog (launch/stop/delete-model) does not touch that
/// decision, so this stays wildcard-only rather than gaining a "read" variant
/// nobody asked for.
///
/// The role test is `Claims::has_wildcard` rather than a substring search on the
/// scope claim: with permissions in the same claim, `contains("admin")` would
/// also match a grant naming a service called `admin`. `system` is gone with it -
/// it was never a role the realm issues.
pub async fn is_admin(req: &actix_web::HttpRequest, config: &web::Data<JwtConfig>) -> bool {
    if !config.auth_enabled {
        return true;
    }

    use actix_web::HttpMessage;
    if let Some(claims) = req
        .extensions()
        .get::<quench_auth::actix::domain::jwt::Claims>()
    {
        return claims.has_wildcard();
    }

    // Fallback for UI if extensions wasn't populated somehow (though Auth middleware should)
    let Some(cookie) = req.cookie(&quench_auth::prelude::realm::session_cookie_name()) else {
        return false;
    };

    match config.decode_claims(cookie.value()).await {
        Ok(claims) => claims.has_wildcard(),
        Err(_) => false,
    }
}

/// Whether the caller may perform `action` on switchboard - `"launch"`,
/// `"stop"` or `"delete-model"`, per `config/permissions.toml`'s catalog entry
/// for this service. A wildcard role satisfies any action without it being
/// granted explicitly, the same as everywhere else `Claims::can` is used.
///
/// This is what replaced the blanket `RequireWrite` middleware for
/// `models::scope`'s and `vllm::scope`'s write routes: those scopes no longer
/// declare a `"write"` action in the catalog at all, on purpose, so nothing
/// there is reachable through a coarse write grant any more - a route needs
/// the specific action this checks.
pub async fn can(req: &actix_web::HttpRequest, config: &web::Data<JwtConfig>, action: &str) -> bool {
    if !config.auth_enabled {
        return true;
    }

    use actix_web::HttpMessage;
    if let Some(claims) = req
        .extensions()
        .get::<quench_auth::actix::domain::jwt::Claims>()
    {
        return claims.can(&config.service_name, action);
    }

    let Some(cookie) = req.cookie(&quench_auth::prelude::realm::session_cookie_name()) else {
        return false;
    };

    match config.decode_claims(cookie.value()).await {
        Ok(claims) => claims.can(&config.service_name, action),
        Err(_) => false,
    }
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
