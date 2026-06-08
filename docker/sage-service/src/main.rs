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

    // Spawn background task to monitor if the default models are available.
    // If not, send a request to switchboard to launch them.
    let switchboard_clone = switchboard.clone();
    let config_clone = config.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(10));
        loop {
            interval.tick().await;
            tracing::debug!("Checking active instances for default models...");
            match switchboard_clone.get_vllm_instances().await {
                Ok(instances) => {
                    for model in &config_clone.default_models {
                        if !config_clone.is_model_supported(&model.name) {
                            tracing::warn!(
                                "Model '{}' is in default_models but does not match any pattern in supported_models.",
                                model.name
                            );
                            continue;
                        }

                        let is_active = instances.iter().any(|inst| {
                            inst.model == model.name
                                && (inst.status == "running"
                                    || inst.status == "starting"
                                    || inst.status == "pending")
                        });

                        if !is_active {
                            tracing::info!(
                                "Default model '{}' is not available. Requesting switchboard to launch it.",
                                model.name
                            );
                            match switchboard_clone
                                .launch_instance(
                                    &model.name,
                                    model.gpu_memory_utilization,
                                    model.max_model_len,
                                )
                                .await
                            {
                                Ok(inst) => {
                                    tracing::info!(
                                        "Successfully requested launch of model '{}'. Instance ID: {}",
                                        model.name,
                                        inst.id
                                    );
                                }
                                Err(err) => {
                                    tracing::error!(
                                        "Failed to request launch of model '{}': {}",
                                        model.name,
                                        err
                                    );
                                }
                            }
                        }
                    }
                }
                Err(err) => {
                    tracing::error!(
                        "Failed to check running instances for default models: {}",
                        err
                    );
                }
            }
        }
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
