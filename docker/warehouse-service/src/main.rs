use quench_auth::prelude::JwtConfig;
use quench_config::ConfigLoader;
use quench_starter::prelude::{DbWrapper, HttpServiceFactory, serve};

pub mod domain;
pub mod middleware;
pub mod routers;
pub mod utils;

pub fn root_scope() -> impl HttpServiceFactory {
    let loader = ConfigLoader::new("WAREHOUSE");
    let max_body_bytes = loader.env_u64("MAX_REQUEST_BODY_BYTES", 1024 * 1024 * 1024) as usize;
    let max_concurrent_uploads = loader.env_u64("MAX_CONCURRENT_UPLOADS", 32) as usize;

    actix_web::web::scope("")
        .app_data(actix_web::web::PayloadConfig::new(max_body_bytes))
        .wrap(middleware::auth::WarehouseAuth::new(JwtConfig::init()))
        .wrap(middleware::limits::WarehouseLimits::new(
            max_concurrent_uploads,
        ))
        .service(routers::docker::scope())
        .service(routers::docker::token::handle)
}

pub fn base_path_scope() -> impl HttpServiceFactory {
    actix_web::web::scope("")
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

    serve(root_scope, base_path_scope, Some(db_wrapper), async {}).await
}
