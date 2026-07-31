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
mod email;
mod realm;
mod services;
mod tokens;
mod ui;

pub use catalog::PermissionCatalog;
pub use tokens::VerificationTokens;

pub fn root_scope() -> impl HttpServiceFactory {
    web::scope("")
}

pub fn base_path_scope(
    jwt_config: web::Data<JwtConfig>,
    user_db: Arc<UserDb>,
    session_db: Arc<SessionDb>,
    db: Db,
    catalog: Arc<PermissionCatalog>,
    mailer: Arc<dyn email::Sender>,
    tokens: Arc<VerificationTokens>,
) -> impl HttpServiceFactory {
    web::scope("")
        .app_data(jwt_config)
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
    let mut jwt_config = JwtConfig::init();
    jwt_config.audiences = catalog.service_names().map(str::to_string).collect();
    if !jwt_config.audiences.contains(&jwt_config.service_name) {
        jwt_config.audiences.push(jwt_config.service_name.clone());
    }

    tracing::info!(
        "Gatehouse starting: realm schema {}, audiences {:?}",
        quench_auth::prelude::realm::auth_schema(),
        jwt_config.audiences
    );

    ui::common::ensure_assets();

    let db_wrapper = DbWrapper::init_env().await;
    let jwt_config = web::Data::new(jwt_config);
    bootstrap::seed_users(&db_wrapper.db).await;
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
