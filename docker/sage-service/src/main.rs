use actix_web::dev::HttpServiceFactory;
use dashmap::DashMap;
use quench_srv::prelude::{DbWrapper, serve, wait_for_services};

mod clients;
mod config;
mod routers;

pub fn root_scope() -> impl HttpServiceFactory {
    routers::root_scope()
}

pub fn base_path_scope(
    switchboard: clients::switchboard::SwitchboardClient,
    vllm: clients::vllm::VllmClient,
    config: config::SageConfig,
    chat_state: actix_web::web::Data<routers::ui::chat::ChatState>,
) -> impl HttpServiceFactory {
    actix_web::web::scope("")
        .app_data(actix_web::web::Data::new(switchboard))
        .app_data(actix_web::web::Data::new(vllm))
        .app_data(actix_web::web::Data::new(config))
        .app_data(chat_state)
        .service(routers::base_path_scope())
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt().init();
    dotenvy::dotenv().ok();

    let switchboard_url = envmnt::get_or("SWITCHBOARD_URL", "http://switchboard-service:8080");
    let health_url = format!("{}/health", switchboard_url.trim_end_matches('/'));
    wait_for_services("sage-service", vec![&health_url]).await;

    let db_wrapper = DbWrapper::init_env().await;
    let switchboard = clients::switchboard::SwitchboardClient::new();
    let vllm = clients::vllm::VllmClient::new();
    let config = config::SageConfig::load();
    let chat_state = actix_web::web::Data::new(routers::ui::chat::ChatState {
        pending_messages: DashMap::new(),
    });

    serve(
        root_scope,
        move || {
            base_path_scope(
                switchboard.clone(),
                vllm.clone(),
                config.clone(),
                chat_state.clone(),
            )
        },
        Some(db_wrapper),
    )
    .await
}
