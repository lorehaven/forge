//! Public keys and rotation - `/.well-known/jwks.json` is unauthenticated,
//! path fixed by RFC 7517.

use crate::api::users::ManageSigningKeysClaims;
use crate::keys::SigningKeys;
use quench_http::prelude::{Inject, Response, get, post};

#[get("/.well-known/jwks.json")]
pub async fn jwks(Inject(keys): Inject<SigningKeys>) -> Response {
    Response::json(quench_http::http::StatusCode::OK, &keys.jwks())
        .unwrap_or_else(|_| Response::new(quench_http::http::StatusCode::INTERNAL_SERVER_ERROR))
}

/// Generates a new signing key, retires the old one. Gated on
/// `gatehouse:manage-signing-keys`.
#[post("/api/v1/admin/keys/rotate")]
pub async fn rotate(
    Inject(keys): Inject<SigningKeys>,
    _claims: ManageSigningKeysClaims,
) -> Response {
    match keys.rotate().await {
        Ok(()) => Response::new(quench_http::http::StatusCode::NO_CONTENT),
        Err(err) => {
            quench_log::error!("key rotation failed: {err}");
            Response::new(quench_http::http::StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub fn register_routes() {
    let _ = jwks as fn(_) -> _;
    let _ = rotate as fn(_, _) -> _;
}
