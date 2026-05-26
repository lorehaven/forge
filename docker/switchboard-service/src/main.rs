use crate::routers::models::{init_model_store, warm_model_cache};
use quench_srv::actix::domain::db::DbWrapper;
use quench_srv::prelude::{HttpServiceFactory, serve};

pub mod routers;

pub fn root_scope() -> impl HttpServiceFactory {
    actix_web::web::scope("")
}

pub fn base_path_scope() -> impl HttpServiceFactory {
    actix_web::web::scope("")
        .service(routers::gpu::scope())
        .service(routers::models::scope())
        .service(routers::vllm::scope())
        .service(routers::ui::scope())
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt().init();
    dotenvy::dotenv().ok();

    let db_wrapper = DbWrapper::init().await;
    init_model_store(db_wrapper.db.clone()).await;
    warm_model_cache().await;

    serve(
        root_scope,
        base_path_scope,
        routers::openapi(),
        Some(db_wrapper),
    )
    .await
}
