//! The estate's public keys, and rotating them.
//!
//! `/.well-known/jwks.json` is unauthenticated and unversioned - every relying
//! party's `JwksVerifier` polls it, and the path is fixed by RFC 7517. Key
//! rotation lives next to it since both are the same `SigningKeys` state.

use crate::api::users::ManageSigningKeysClaims;
use crate::keys::SigningKeys;
use actix_web::{HttpResponse, Responder, get, post, web};
use std::sync::Arc;

#[get("/.well-known/jwks.json")]
pub async fn jwks(keys: web::Data<Arc<SigningKeys>>) -> impl Responder {
    HttpResponse::Ok().json(keys.jwks())
}

/// Generates a new signing key and retires the current one - see
/// `SigningKeys::rotate` for what "retires" means for tokens already out
/// there. Gated on `gatehouse:manage-signing-keys`, one of the few catalog
/// actions delegable below the literal `admin` role.
#[post("/api/v1/admin/keys/rotate")]
pub async fn rotate(
    keys: web::Data<Arc<SigningKeys>>,
    _claims: ManageSigningKeysClaims,
) -> impl Responder {
    match keys.rotate().await {
        Ok(()) => HttpResponse::NoContent().finish(),
        Err(err) => {
            tracing::error!("key rotation failed: {err}");
            HttpResponse::InternalServerError().finish()
        }
    }
}
