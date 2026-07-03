use actix_web::web;
use quench_auth::prelude::{JwtConfig, SessionDb, UserDb};
use quench_config::ConfigLoader;
use quench_starter::prelude::{DbWrapper, HttpServiceFactory, serve};
use std::sync::Arc;

pub mod domain;
pub mod middleware;
pub mod routers;
pub mod utils;

pub fn root_scope(
    jwt_config: web::Data<JwtConfig>,
    user_db: Arc<UserDb>,
    session_db: Arc<SessionDb>,
) -> impl HttpServiceFactory {
    let loader = ConfigLoader::new("WAREHOUSE");
    let max_body_bytes = loader.env_u64("MAX_REQUEST_BODY_BYTES", 1024 * 1024 * 1024) as usize;
    let max_concurrent_uploads = loader.env_u64("MAX_CONCURRENT_UPLOADS", 32) as usize;

    web::scope("")
        .app_data(actix_web::web::PayloadConfig::new(max_body_bytes))
        .app_data(jwt_config.clone())
        .app_data(web::Data::new(user_db))
        .app_data(web::Data::new(session_db))
        .wrap(middleware::auth::WarehouseAuth::new(jwt_config.get_ref().clone()))
        .wrap(middleware::limits::WarehouseLimits::new(
            max_concurrent_uploads,
        ))
        .service(routers::docker::scope())
        .service(routers::docker::token::handle)
}

pub fn base_path_scope(
    jwt_config: web::Data<JwtConfig>,
    user_db: Arc<UserDb>,
    session_db: Arc<SessionDb>,
) -> impl HttpServiceFactory {
    web::scope("")
        .app_data(jwt_config)
        .app_data(web::Data::new(user_db))
        .app_data(web::Data::new(session_db))
        .service(routers::admin::scope())
        .service(routers::crates::scope())
        .service(routers::crates::scope_index())
        .service(routers::ui::scope())
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    quench_starter::logging::init();
    tracing::info!("Warehouse service starting");

    let db_wrapper = DbWrapper::init_env().await;
    let jwt_config = web::Data::new(JwtConfig::init());
    let user_db = UserDb::init(db_wrapper.db.clone()).await;
    let session_db = SessionDb::init(db_wrapper.db.clone());

    let root_user_db = user_db.clone();
    let root_session_db = session_db.clone();
    let root_jwt_config = jwt_config.clone();

    serve(
        move || root_scope(root_jwt_config.clone(), root_user_db.clone(), root_session_db.clone()),
        move || {
            base_path_scope(
                jwt_config.clone(),
                user_db.clone(),
                session_db.clone(),
            )
        },
        Some(db_wrapper),
        async {},
    )
    .await
}
