use quench_srv::prelude::{serve, HttpServiceFactory};

pub mod domain;
pub mod middleware;
pub mod routers;
pub mod utils;

pub fn root_scope() -> impl HttpServiceFactory {
    let max_body_bytes: usize = envmnt::get_or("MAX_REQUEST_BODY_BYTES", "1073741824")
        .parse()
        .unwrap_or(1024 * 1024 * 1024);
    let max_concurrent_uploads: usize = envmnt::get_or("MAX_CONCURRENT_UPLOADS", "32")
        .parse()
        .unwrap_or(32);

    actix_web::web::scope("")
        .app_data(actix_web::web::PayloadConfig::new(max_body_bytes))
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
        .service(routers::files::scope())
        .service(routers::ui::scope())
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt().init();
    dotenvy::dotenv().ok();

    serve(root_scope, base_path_scope, routers::openapi()).await
}
