use actix_web::web;
use quench_srv::prelude::JwtConfig;
use std::sync::LazyLock;

pub static HF_ROOTS: LazyLock<Vec<String>> =
    LazyLock::new(|| load_paths("HF_ROOTS", &["/mnt/dev/huggingface/hub"]));

pub static GGUF_ROOTS: LazyLock<Vec<String>> =
    LazyLock::new(|| load_paths("GGUF_ROOTS", &["/mnt/dev/quantized"]));

pub fn is_admin(req: &actix_web::HttpRequest, config: &web::Data<JwtConfig>) -> bool {
    if !config.auth_enabled {
        return true;
    }

    let cookie_name = format!("{}_ui_session", config.service_name);
    let Some(cookie) = req.cookie(&cookie_name) else {
        return false;
    };

    match config.decode_claims(cookie.value()) {
        Ok(claims) => claims.scope.contains("admin") || claims.scope.contains("system"),
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
