//! Gatehouse - the authentication service for the Forge estate.
//!
//! Owns the `auth` schema: it is the only service that seeds users, hosts the
//! only login form, and issues the tokens every other service verifies. Relying
//! parties keep verifying locally (no call to gatehouse on the hot path); they
//! send a browser here only when there is no valid session.

use actix_web::web;
use quench_auth::prelude::{JwtConfig, SessionDb, UserDb};
use quench_db::prelude::Db;
use quench_starter::prelude::HttpServiceFactory;
use std::sync::Arc;

pub mod api;
pub mod bootstrap;
pub mod catalog;
pub mod clients;
pub mod codes;
pub mod crypto;
pub mod email;
pub mod keys;
pub mod mfa;
pub mod realm;
pub mod services;
pub mod test_support;
pub mod tokens;
pub mod ui;

pub use catalog::PermissionCatalog;
pub use keys::SigningKeys;
pub use tokens::VerificationTokens;

pub fn root_scope() -> impl HttpServiceFactory {
    web::scope("")
}

#[allow(clippy::too_many_arguments)]
pub fn base_path_scope(
    jwt_config: web::Data<JwtConfig>,
    signing_keys: web::Data<Arc<SigningKeys>>,
    user_db: Arc<UserDb>,
    session_db: Arc<SessionDb>,
    db: Db,
    catalog: Arc<PermissionCatalog>,
    mailer: Arc<dyn email::Sender>,
    tokens: Arc<VerificationTokens>,
) -> impl HttpServiceFactory {
    web::scope("")
        .app_data(jwt_config)
        .app_data(signing_keys)
        .app_data(web::Data::new(user_db))
        // The auth API takes `Data<SessionDb>` and the UI pages take
        // `Data<Arc<SessionDb>>`; register both views of the same instance.
        .app_data(web::Data::from(session_db.clone()))
        .app_data(web::Data::new(session_db))
        // The user API writes to `auth.users`, so it needs the database rather
        // than the read-only `UserDb` every other service gets.
        .app_data(web::Data::new(db))
        // `Data::from`, not `Data::new`: `catalog` is already an `Arc`, and
        // every handler asks for bare `Data<PermissionCatalog>` - wrapping it
        // again would register `Data<Arc<PermissionCatalog>>` instead, a type
        // nothing asks for, which fails at request time rather than at
        // compile time (`web::Data<T>: FromRequest` for any `T`, so the
        // mismatch does not show up until a handler actually runs).
        .app_data(web::Data::from(catalog))
        .app_data(web::Data::new(mailer))
        .app_data(web::Data::new(tokens))
        .service(api::auth::scope())
        // Registered as bare handlers, not `web::scope("")`: actix's scope
        // resolution treats an empty prefix as matching every path, so the
        // first such scope permanently claims routing and 404s internally
        // for anything not defined inside it - every sibling `.service(...)`
        // registered after it, including `ui::scope()`, would silently
        // become unreachable.
        .service(api::jwks::jwks)
        .service(api::jwks::rotate)
        .service(api::oauth::authorize)
        .service(api::oauth::token)
        .service(api::test_tokens::mint)
        .service(api::users::scope())
        .service(api::users::me_scope())
        .service(ui::scope())
}
