//! Gatehouse - the authentication service for the Forge estate.
//!
//! Owns the `auth` schema: it is the only service that seeds users, hosts the
//! only login form, and issues the tokens every other service verifies. Relying
//! parties keep verifying locally (no call to gatehouse on the hot path); they
//! send a browser here only when there is no valid session.

use actix_web::web;
use quench_auth::prelude::{JwtConfig, SessionDb, UserDb};
use quench_db::prelude::Db;
use quench_starter::prelude::{DbWrapper, HttpServiceFactory, serve};
use std::sync::Arc;

mod api;
mod bootstrap;
mod catalog;
mod clients;
mod codes;
mod email;
mod keys;
mod realm;
mod services;
mod tokens;
mod ui;

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

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    quench_starter::logging::init();

    envmnt::set("SERVICE_NAME", envmnt::get_or("SERVICE_NAME", "gatehouse"));
    // Gatehouse owns the realm, so it is the one service that creates users.
    envmnt::set("AUTH_BOOTSTRAP", envmnt::get_or("AUTH_BOOTSTRAP", "true"));

    let catalog = Arc::new(PermissionCatalog::load().expect(
        "permission catalog failed to load - see PERMISSIONS_CONFIG \
         (default config/permissions.toml)",
    ));

    ui::common::ensure_assets();

    let db_wrapper = DbWrapper::init_env().await;

    // Every token gatehouse issues is signed with a key from here - see
    // `keys.rs`. Retiring a rotated-out key after one access-token TTL means
    // an outstanding token keeps verifying for the rest of its life.
    let access_token_ttl_secs = envmnt::get_or("ACCESS_TOKEN_TTL_SECS", "900")
        .parse()
        .unwrap_or(900);
    let signing_keys = keys::SigningKeys::init(db_wrapper.db.clone(), access_token_ttl_secs)
        .await
        .expect("failed to load or generate gatehouse's signing keys");

    // The catalog's service list is the realm's audience list now: it is what
    // decides which services a token can be issued for, same as
    // `SERVICE_AUDIENCES` used to, but it cannot drift from what is actually
    // grantable because there is only the one list.
    //
    // Gatehouse itself is added explicitly. It is not a grantable service in
    // the catalog - "admin" grants gatehouse's own admin pages, not a
    // service:action pair - but a wildcard user's token skips per-user
    // narrowing entirely (see `user_audiences`) and gets this ceiling
    // verbatim, so gatehouse missing from it would lock every admin out of
    // gatehouse's own admin API on their next token.
    let mut jwt_config = JwtConfig::init_signing(signing_keys.clone());
    jwt_config.audiences = catalog.service_names().map(str::to_string).collect();
    if !jwt_config.audiences.contains(&jwt_config.service_name) {
        jwt_config.audiences.push(jwt_config.service_name.clone());
    }

    tracing::info!(
        "Gatehouse starting: realm schema {}, audiences {:?}",
        quench_auth::prelude::realm::auth_schema(),
        jwt_config.audiences
    );

    let jwt_config = web::Data::new(jwt_config);
    let signing_keys = web::Data::new(signing_keys);
    bootstrap::seed_users(&db_wrapper.db).await;
    if let Err(err) = clients::seed_clients(&db_wrapper.db).await {
        tracing::error!(
            "failed to seed OAuth clients from CLIENTS_CONFIG: {err} - the \
             authorization-code and client_credentials grants will reject every client"
        );
    }
    let user_db = UserDb::init(db_wrapper.db.clone()).await;

    // Sessions live in the cache store, not the database: expiry is its TTL and
    // revocation is a delete, so there is nothing to migrate or sweep.
    let session_db = SessionDb::from_env()
        .await
        .expect("session store unavailable");

    let db = db_wrapper.db.clone();
    // The only sender that exists today - see `email`'s module docs for why
    // that is deliberate, and what replacing it later looks like.
    let mailer: Arc<dyn email::Sender> = Arc::new(email::LoggingSender);
    let tokens = Arc::new(
        VerificationTokens::from_env()
            .await
            .expect("verification token store unavailable"),
    );

    serve(
        root_scope,
        move || {
            base_path_scope(
                jwt_config.clone(),
                signing_keys.clone(),
                user_db.clone(),
                session_db.clone(),
                db.clone(),
                catalog.clone(),
                mailer.clone(),
                tokens.clone(),
            )
        },
        Some(db_wrapper),
        async {},
    )
    .await
}
