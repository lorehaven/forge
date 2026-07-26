//! Gatehouse - the authentication service for the Forge estate.
//!
//! Owns the `auth` schema: it is the only service that seeds users, hosts the
//! only login form, and issues the tokens every other service verifies. Relying
//! parties keep verifying locally (no call to gatehouse on the hot path); they
//! send a browser here only when there is no valid session.

use actix_web::web;
use quench_auth::prelude::{JwtConfig, SessionDb, UserDb};
use quench_starter::prelude::{DbWrapper, HttpServiceFactory, serve};
use std::sync::Arc;

mod api;
mod bootstrap;
mod services;
mod ui;

pub fn root_scope() -> impl HttpServiceFactory {
    web::scope("")
}

pub fn base_path_scope(
    jwt_config: web::Data<JwtConfig>,
    user_db: Arc<UserDb>,
    session_db: Arc<SessionDb>,
) -> impl HttpServiceFactory {
    web::scope("")
        .app_data(jwt_config)
        .app_data(web::Data::new(user_db))
        // The auth API takes `Data<SessionDb>` and the UI pages take
        // `Data<Arc<SessionDb>>`; register both views of the same instance.
        .app_data(web::Data::from(session_db.clone()))
        .app_data(web::Data::new(session_db))
        .service(api::auth::scope())
        .service(ui::scope())
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    quench_starter::logging::init();

    envmnt::set("SERVICE_NAME", envmnt::get_or("SERVICE_NAME", "gatehouse"));
    // Gatehouse owns the realm, so it is the one service that creates users.
    envmnt::set("AUTH_BOOTSTRAP", envmnt::get_or("AUTH_BOOTSTRAP", "true"));

    tracing::info!(
        "Gatehouse starting: realm schema {}, audiences {:?}",
        quench_auth::prelude::realm::auth_schema(),
        JwtConfig::init().audiences
    );

    ui::common::ensure_assets();

    let db_wrapper = DbWrapper::init_env().await;
    let jwt_config = web::Data::new(JwtConfig::init());
    bootstrap::seed_users(&db_wrapper.db).await;
    let user_db = UserDb::init(db_wrapper.db.clone()).await;

    // Sessions live in the cache store, not the database: expiry is its TTL and
    // revocation is a delete, so there is nothing to migrate or sweep.
    let session_db = SessionDb::from_env()
        .await
        .expect("session store unavailable");

    serve(
        root_scope,
        move || base_path_scope(jwt_config.clone(), user_db.clone(), session_db.clone()),
        Some(db_wrapper),
        async {},
    )
    .await
}
