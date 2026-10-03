//! `GET /api/v1/info` - what is running, and who the caller is to it.

use crate::routers::api::{OptionalClaims, authz};
use quench_auth::domain::jwt::JwtConfig;
use quench_http::prelude::{Inject, Response, get, http::StatusCode};
use serde_json::json;

#[get("/api/v1/info")]
pub async fn info(
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
) -> Response {
    let actions = ["read", "deploy", "scale", "activate", "rollback"];
    let allowed: Vec<&str> = actions
        .into_iter()
        .filter(|action| authz::can(claims.as_ref(), &config, action))
        .collect();

    Response::json(
        StatusCode::OK,
        &json!({
            "service": "gantry",
            "version": env!("CARGO_PKG_VERSION"),
            "actor": super::actor(claims.as_ref()),
            "can": allowed,
        }),
    )
    .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
}

pub fn register_routes() {
    let _ = info as fn(_, _) -> _;
}
