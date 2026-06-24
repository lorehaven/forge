use actix_web::dev::HttpServiceFactory;
use dashmap::DashMap;
use quench_srv::prelude::JwtConfig;
use quench_srv::prelude::{DbWrapper, serve, wait_for_services};

mod audit;
mod clients;
mod config;
pub mod conversation;
pub mod error_handling;
pub mod metrics;
pub mod models;
pub mod rate_limiter;
pub mod response_cache;
mod routers;
pub mod tools;

pub fn root_scope() -> impl HttpServiceFactory {
    routers::root_scope()
}

pub fn base_path_scope(
    switchboard: clients::switchboard::SwitchboardClient,
    vllm: clients::vllm::VllmClient,
    config: config::SageConfig,
    chat_state: actix_web::web::Data<routers::ui::chat::ChatState>,
    jwt_config: JwtConfig,
    tool_registry: actix_web::web::Data<tools::ToolRegistry>,
    search_provider_registry: actix_web::web::Data<std::sync::Arc<tools::SearchProviderRegistry>>,
    metrics_collector: actix_web::web::Data<std::sync::Arc<metrics::MetricsCollector>>,
    rate_limiter: actix_web::web::Data<std::sync::Arc<tokio::sync::Mutex<rate_limiter::RateLimiter>>>,
) -> impl HttpServiceFactory {
    let response_cache = actix_web::web::Data::new(response_cache::ResponseCache::new());

    actix_web::web::scope("")
        .app_data(actix_web::web::Data::new(switchboard))
        .app_data(actix_web::web::Data::new(vllm))
        .app_data(actix_web::web::Data::new(config))
        .app_data(chat_state)
        .app_data(tool_registry)
        .app_data(search_provider_registry)
        .app_data(metrics_collector)
        .app_data(rate_limiter)
        .app_data(response_cache)
        .service(routers::base_path_scope(jwt_config))
}

fn init_tracing() {
    tracing_subscriber::fmt().init();
    dotenvy::dotenv().ok();
    envmnt::set("DB_SCHEMA", envmnt::get_or("DB_SCHEMA", "sage"));
}

fn get_health_check_url() -> String {
    let switchboard_url = envmnt::get_or("SWITCHBOARD_URL", "http://switchboard-service:8080");
    format!("{}/health/ready", switchboard_url.trim_end_matches('/'))
}

async fn init_database() -> std::sync::Arc<DbWrapper> {
    DbWrapper::init_env().await
}

fn init_clients() -> (
    clients::switchboard::SwitchboardClient,
    clients::vllm::VllmClient,
) {
    (
        clients::switchboard::SwitchboardClient::new(),
        clients::vllm::VllmClient::new(),
    )
}

fn init_config() -> (config::SageConfig, JwtConfig) {
    (config::SageConfig::load(), JwtConfig::init())
}

fn init_chat_state() -> actix_web::web::Data<routers::ui::chat::ChatState> {
    actix_web::web::Data::new(routers::ui::chat::ChatState {
        pending_messages: DashMap::new(),
    })
}

fn init_search_provider_registry() -> std::sync::Arc<tools::SearchProviderRegistry> {
    let mut registry = tools::SearchProviderRegistry::new();

    registry.register(
        "duckduckgo".to_string(),
        Box::new(tools::search_providers::DuckDuckGoProvider::new()),
    );

    if let Ok(brave_provider) = tools::search_providers::BraveProvider::from_env() {
        registry.register("brave".to_string(), Box::new(brave_provider));
        tracing::info!("Brave Search provider registered");
    } else {
        tracing::info!("Brave Search API key not set, provider not available");
    }

    if let Ok(searxng_provider) = tools::search_providers::SearxngProvider::from_env() {
        registry.register("searxng".to_string(), Box::new(searxng_provider));
        tracing::info!("SearXNG provider registered");
    } else {
        tracing::info!("SearXNG instance URL not set or invalid, using default");
        registry.register(
            "searxng".to_string(),
            Box::new(tools::search_providers::SearxngProvider::new(
                "https://searxng.be".to_string(),
            )),
        );
        tracing::info!("SearXNG provider registered with default instance");
    }

    if let Ok(serpapi_provider) = tools::search_providers::SerpapiProvider::from_env() {
        registry.register("serpapi".to_string(), Box::new(serpapi_provider));
        tracing::info!("SerpAPI provider registered");
    } else {
        tracing::info!("SerpAPI API key not set, provider not available");
    }

    let default_provider = envmnt::get_or("SEARCH_PROVIDER", "duckduckgo");
    registry.set_default(default_provider);

    std::sync::Arc::new(registry)
}

fn init_metrics_collector() -> std::sync::Arc<metrics::MetricsCollector> {
    std::sync::Arc::new(metrics::MetricsCollector::new())
}

fn init_rate_limiter() -> std::sync::Arc<tokio::sync::Mutex<rate_limiter::RateLimiter>> {
    std::sync::Arc::new(tokio::sync::Mutex::new(rate_limiter::RateLimiter::new()))
}

fn init_tool_registry(
    search_provider_registry: &std::sync::Arc<tools::SearchProviderRegistry>,
    profile: &tools::CapabilityProfile,
) -> actix_web::web::Data<tools::ToolRegistry> {
    let mut registry = tools::ToolRegistry::with_profile(profile.clone());

    registry.register(
        "web_search".to_string(),
        Box::new(tools::web_search::WebSearchExecutor::new(
            search_provider_registry.clone(),
        )),
    );

    registry.register(
        "calculator".to_string(),
        Box::new(tools::calculator::CalculatorExecutor),
    );

    registry.register(
        "web_fetch".to_string(),
        Box::new(tools::web_fetch::WebFetchExecutor::new()),
    );

    registry.register(
        "file_ops".to_string(),
        Box::new(tools::file_ops::FileOpsExecutor::from_env()),
    );

    registry.register(
        "command".to_string(),
        Box::new(tools::command::CommandExecutor::new()),
    );

    registry.register(
        "code_executor".to_string(),
        Box::new(tools::code_executor::CodeExecutor),
    );

    actix_web::web::Data::new(registry)
}

fn spawn_model_monitor_task(
    switchboard: clients::switchboard::SwitchboardClient,
    config: config::SageConfig,
) {
    let switchboard_clone = switchboard.clone();
    let config_clone = config.clone();

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(10));
        loop {
            interval.tick().await;
            monitor_default_models(&switchboard_clone, &config_clone).await;
        }
    });
}

async fn monitor_default_models(
    switchboard: &clients::switchboard::SwitchboardClient,
    config: &config::SageConfig,
) {
    tracing::debug!("Checking active instances for default models...");

    let instances = match switchboard.get_vllm_instances().await {
        Ok(inst) => inst,
        Err(err) => {
            tracing::error!(
                "Failed to check running instances for default models: {}",
                err
            );
            return;
        }
    };

    for model in &config.default_models {
        if !config.is_model_supported(&model.name) {
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
            request_model_launch(switchboard, model).await;
        }
    }
}

async fn request_model_launch(
    switchboard: &clients::switchboard::SwitchboardClient,
    model: &config::DefaultModel,
) {
    tracing::info!(
        "Default model '{}' is not available. Requesting switchboard to launch it.",
        model.name
    );

    match switchboard
        .launch_instance(
            &model.name,
            model.gpu_memory_utilization,
            model.max_model_len,
            model.enable_tool_calling,
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

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    init_tracing();

    let health_url = get_health_check_url();
    let db_wrapper = init_database().await;
    let (switchboard, vllm) = init_clients();
    let (sage_config, jwt_config) = init_config();
    let chat_state = init_chat_state();
    let search_provider_registry = init_search_provider_registry();
    let metrics_collector = init_metrics_collector();
    let rate_limiter = init_rate_limiter();
    let tool_registry =
        init_tool_registry(&search_provider_registry, &sage_config.capability_profile);

    spawn_model_monitor_task(switchboard.clone(), sage_config.clone());

    serve(
        root_scope,
        move || {
            base_path_scope(
                switchboard.clone(),
                vllm.clone(),
                sage_config.clone(),
                chat_state.clone(),
                jwt_config.clone(),
                tool_registry.clone(),
                actix_web::web::Data::new(search_provider_registry.clone()),
                actix_web::web::Data::new(metrics_collector.clone()),
                actix_web::web::Data::new(rate_limiter.clone()),
            )
        },
        Some(db_wrapper),
        async move {
            wait_for_services("sage-service", vec![health_url.as_str()]).await;
        },
    )
    .await
}
