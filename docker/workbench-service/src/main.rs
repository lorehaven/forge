use actix_web::web;
use quench_auth::actix::domain::sso_client::SsoConfig;
use quench_auth::prelude::{JwtConfig, SessionDb, UserDb};
use quench_starter::prelude::{DbWrapper, gatehouse_health_url, serve, wait_for_services};
use workbench_service::base_path_scope;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    quench_starter::logging::init();
    tracing::info!("Workbench service starting");

    let db_wrapper = DbWrapper::init_env().await;
    let jwt_config = web::Data::new(JwtConfig::init().await);
    let sso_config = web::Data::new(SsoConfig::init());
    let user_db = UserDb::init(db_wrapper.db.clone()).await;
    let session_db = SessionDb::from_env().await.expect("session store");

    serve(
        || actix_web::web::scope(""),
        move || {
            base_path_scope(
                jwt_config.clone(),
                sso_config.clone(),
                user_db.clone(),
                session_db.clone(),
            )
        },
        Some(db_wrapper),
        async move {
            wait_for_services("workbench-service", vec![gatehouse_health_url().as_str()]).await;
        },
    )
    .await
}
