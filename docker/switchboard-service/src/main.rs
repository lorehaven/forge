use crate::routers::models::warm_model_cache;
use quench_srv::prelude::{HttpServiceFactory, serve};

pub mod routers;

pub fn root_scope() -> impl HttpServiceFactory {
    actix_web::web::scope("")
}

pub fn base_path_scope() -> impl HttpServiceFactory {
    actix_web::web::scope("")
        .service(routers::gpu::scope())
        .service(routers::models::scope())
        .service(routers::ui::scope())
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt().init();
    dotenvy::dotenv().ok();
    warm_model_cache();

    serve(root_scope, base_path_scope, routers::openapi()).await
}
