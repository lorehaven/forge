use actix_web::web;
use quench_auth::actix::domain::sso_client::SsoConfig;
use quench_auth::prelude::{JwtConfig, SessionDb, UserDb};
use quench_starter::prelude::{DbWrapper, serve};
use warehouse_service::docker_token::DockerTokenConfig;
use warehouse_service::{base_path_scope, root_scope};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    quench_starter::logging::init();
    tracing::info!("Warehouse service starting");

    let db_wrapper = DbWrapper::init_env().await;
    let jwt_config = web::Data::new(JwtConfig::init().await);
    let docker_token_config = web::Data::new(DockerTokenConfig::init(
        jwt_config.service_name.clone(),
        jwt_config.realm.clone(),
        jwt_config.auth_enabled,
    ));
    let sso_config = web::Data::new(SsoConfig::init());
    let user_db = UserDb::init(db_wrapper.db.clone()).await;
    let session_db = SessionDb::from_env().await.expect("session store");

    // Reported at startup rather than discovered by a caller getting a 404 for
    // a storage this deployment was never given.
    warehouse_service::routers::files::report_storages();

    let root_user_db = user_db.clone();
    let root_session_db = session_db.clone();
    let root_jwt_config = jwt_config.clone();
    let root_docker_token_config = docker_token_config.clone();

    serve(
        move || {
            root_scope(
                root_jwt_config.clone(),
                root_docker_token_config.clone(),
                root_user_db.clone(),
                root_session_db.clone(),
            )
        },
        move || {
            base_path_scope(
                jwt_config.clone(),
                sso_config.clone(),
                user_db.clone(),
                session_db.clone(),
            )
        },
        Some(db_wrapper),
        async {},
    )
    .await
}
