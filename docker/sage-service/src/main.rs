use actix_web::dev::HttpServiceFactory;
use dashmap::DashMap;
use quench_auth::prelude::*;
use quench_starter::prelude::*;

mod audit;
mod clients;
mod config;
pub mod context_manager;
pub mod conversation;
pub mod cost_tracking;
pub mod files;
mod init;
pub mod metrics;
pub mod models;
pub mod rate_limiter;
pub mod retry_policy;
mod routers;
pub mod tools;

pub fn root_scope() -> impl HttpServiceFactory {
    routers::root_scope()
}

#[allow(clippy::too_many_arguments)]
pub fn base_path_scope(
    switchboard: clients::switchboard::SwitchboardClient,
    vllm: clients::vllm::VllmClient,
    config: config::SageConfig,
    chat_state: actix_web::web::Data<routers::ui::chat::ChatState>,
    jwt_config: actix_web::web::Data<JwtConfig>,
    user_db: std::sync::Arc<UserDb>,
    session_db: std::sync::Arc<SessionDb>,
    tool_registry: actix_web::web::Data<tools::ToolRegistry>,
    search_provider_registry: actix_web::web::Data<std::sync::Arc<tools::SearchProviderRegistry>>,
    metrics_collector: actix_web::web::Data<std::sync::Arc<metrics::MetricsCollector>>,
    rate_limiter: actix_web::web::Data<
        std::sync::Arc<tokio::sync::Mutex<rate_limiter::RateLimiter>>,
    >,
    cost_tracker: actix_web::web::Data<std::sync::Arc<cost_tracking::CostTracker>>,
) -> impl HttpServiceFactory {
    let jwt_config_for_router = jwt_config.get_ref().clone();
    actix_web::web::scope("")
        .app_data(actix_web::web::Data::new(switchboard))
        .app_data(actix_web::web::Data::new(vllm))
        .app_data(actix_web::web::Data::new(config))
        .app_data(chat_state)
        .app_data(jwt_config)
        .app_data(actix_web::web::Data::new(user_db))
        .app_data(actix_web::web::Data::new(session_db))
        .app_data(tool_registry)
        .app_data(search_provider_registry)
        .app_data(metrics_collector)
        .app_data(rate_limiter)
        .app_data(cost_tracker)
        .service(routers::base_path_scope(jwt_config_for_router))
}

fn init_tracing() {
    quench_starter::logging::init();
    tracing::info!("Sage service starting");
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

fn init_cost_tracker() -> std::sync::Arc<cost_tracking::CostTracker> {
    std::sync::Arc::new(cost_tracking::CostTracker::new())
}

fn init_tool_registry(
    search_provider_registry: &std::sync::Arc<tools::SearchProviderRegistry>,
    profile: &tools::CapabilityProfile,
    db: quench_db::prelude::Db,
    switchboard: clients::switchboard::SwitchboardClient,
    vllm: clients::vllm::VllmClient,
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

    // Registered without a conversation context; the chat flow builds a
    // request-scoped registry that carries the actual conversation id.
    registry.register(
        "file_search".to_string(),
        Box::new(tools::file_search::FileSearchExecutor::new(
            db.clone(),
            switchboard,
            vllm,
            None,
            None,
        )),
    );

    // Registered without a conversation context; the chat flow builds a
    // request-scoped registry that carries the actual conversation id.
    registry.register(
        "file_list".to_string(),
        Box::new(tools::file_list::FileListExecutor::new(db, None, None)),
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

/// How many times the monitor retries a default model that never reaches
/// `running` before it gives up and moves on to the next one. Without this a
/// single unlaunchable model would block every model behind it forever, since
/// launches are serialized.
const MAX_LAUNCH_ATTEMPTS: u32 = 3;

fn spawn_model_monitor_task(
    switchboard: clients::switchboard::SwitchboardClient,
    config: config::SageConfig,
) {
    let switchboard_clone = switchboard.clone();
    let config_clone = config.clone();

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(10));
        // Consecutive launch attempts per model, kept across ticks so a model
        // that keeps failing does not stall the rest of the queue.
        let mut attempts: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
        loop {
            interval.tick().await;
            monitor_default_models(&switchboard_clone, &config_clone, &mut attempts).await;
        }
    });
}

/// Instance is still coming up: weights loading, pod scheduling, etc.
fn is_starting(status: &str) -> bool {
    matches!(status, "starting" | "pending")
}

/// Instance counts as covering its model — either up or on its way up.
fn is_active(status: &str) -> bool {
    status == "running" || is_starting(status)
}

/// Pick the next default model to launch. Models are launched **one at a
/// time**: while any instance is still starting this returns `None`, so the
/// next launch waits until the current one has finished loading (or failed)
/// rather than having several models contend for GPU memory at once.
fn next_model_to_launch<'a>(
    config: &'a config::SageConfig,
    instances: &[clients::switchboard::VllmInstance],
    attempts: &std::collections::HashMap<String, u32>,
) -> Option<&'a config::DefaultModel> {
    if let Some(inst) = instances.iter().find(|inst| is_starting(&inst.status)) {
        tracing::debug!(
            "Instance '{}' is still starting; holding off on launching the next default model.",
            inst.model
        );
        return None;
    }

    config.default_models.iter().find(|model| {
        if !config.is_model_supported(&model.name) {
            return false;
        }
        if attempts
            .get(&model.name)
            .is_some_and(|n| *n >= MAX_LAUNCH_ATTEMPTS)
        {
            return false;
        }
        !instances
            .iter()
            .any(|inst| inst.model == model.name && is_active(&inst.status))
    })
}

async fn monitor_default_models(
    switchboard: &clients::switchboard::SwitchboardClient,
    config: &config::SageConfig,
    attempts: &mut std::collections::HashMap<String, u32>,
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
        // A model that came up successfully starts over with a clean slate, so
        // a later restart is retried rather than treated as exhausted.
        if instances
            .iter()
            .any(|inst| inst.model == model.name && inst.status == "running")
        {
            attempts.remove(&model.name);
        }
    }

    let Some(model) = next_model_to_launch(config, &instances, attempts) else {
        return;
    };

    let attempt = attempts.entry(model.name.clone()).or_insert(0);
    *attempt += 1;
    let attempt = *attempt;

    request_model_launch(switchboard, model).await;

    if attempt >= MAX_LAUNCH_ATTEMPTS {
        tracing::error!(
            "This was attempt {} of {} for default model '{}'. If it does not reach \
             'running' it will be skipped so the remaining default models can start.",
            attempt,
            MAX_LAUNCH_ATTEMPTS,
            model.name
        );
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
            model.quantization.as_deref(),
            model.dtype.as_deref(),
            model.limit_mm_per_prompt.as_deref(),
            model.enable_tool_calling,
            model.task.as_deref(),
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

/// Gracefully stop the default models on service shutdown. Fetches the active
/// instances and asks switchboard to SIGTERM each one that corresponds to a
/// configured default model. Best-effort: failures are logged, not fatal.
async fn shutdown_default_models(
    switchboard: &clients::switchboard::SwitchboardClient,
    config: &config::SageConfig,
) {
    tracing::info!("Stopping default models on shutdown...");

    let instances = match switchboard.get_vllm_instances().await {
        Ok(inst) => inst,
        Err(err) => {
            tracing::error!(
                "Failed to list instances while stopping default models: {}",
                err
            );
            return;
        }
    };

    for model in &config.default_models {
        for inst in instances.iter().filter(|inst| {
            inst.model == model.name
                && matches!(inst.status.as_str(), "running" | "starting" | "pending")
        }) {
            match switchboard.stop_instance(&inst.id).await {
                Ok(_) => tracing::info!(
                    "Requested graceful stop of default model '{}' (instance {})",
                    model.name,
                    inst.id
                ),
                Err(err) => tracing::error!(
                    "Failed to stop default model '{}' (instance {}): {}",
                    model.name,
                    inst.id,
                    err
                ),
            }
        }
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    init_tracing();
    envmnt::set("DB_SCHEMA", envmnt::get_or("DB_SCHEMA", "sage"));

    let health_url = get_health_check_url();
    let db_wrapper = init_database().await;
    let (switchboard, vllm) = init_clients();
    let (sage_config, jwt_config) = init_config();
    let user_db = UserDb::init(db_wrapper.db.clone()).await;
    let session_db = SessionDb::init(db_wrapper.db.clone());
    let chat_state = init_chat_state();
    let search_provider_registry = init_search_provider_registry();
    let metrics_collector = init_metrics_collector();
    let rate_limiter = init_rate_limiter();
    let cost_tracker = init_cost_tracker();
    let tool_registry = init_tool_registry(
        &search_provider_registry,
        &sage_config.capability_profile,
        db_wrapper.db.clone(),
        switchboard.clone(),
        vllm.clone(),
    );
    let jwt_config_data = actix_web::web::Data::new(jwt_config);

    spawn_model_monitor_task(switchboard.clone(), sage_config.clone());

    // Clones retained for graceful shutdown after the server stops; the
    // originals below are moved into the server factory closure.
    let shutdown_switchboard = switchboard.clone();
    let shutdown_config = sage_config.clone();

    // Validate critical dependencies at startup
    if let Err(e) = init::validate_startup(&switchboard, &sage_config).await {
        tracing::error!("Startup validation failed: {}", e);
        tracing::error!("The service may not function correctly. Please check your configuration.");
    }

    let result = serve(
        root_scope,
        move || {
            base_path_scope(
                switchboard.clone(),
                vllm.clone(),
                sage_config.clone(),
                chat_state.clone(),
                jwt_config_data.clone(),
                user_db.clone(),
                session_db.clone(),
                tool_registry.clone(),
                actix_web::web::Data::new(search_provider_registry.clone()),
                actix_web::web::Data::new(metrics_collector.clone()),
                actix_web::web::Data::new(rate_limiter.clone()),
                actix_web::web::Data::new(cost_tracker.clone()),
            )
        },
        Some(db_wrapper),
        async move {
            wait_for_services("sage-service", vec![health_url.as_str()]).await;
        },
    )
    .await;

    // The server future resolves once actix has completed its graceful
    // shutdown (e.g. on SIGTERM/SIGINT). Optionally tear down the default
    // models we asked switchboard to launch at startup.
    if shutdown_config.stop_models_on_shutdown {
        shutdown_default_models(&shutdown_switchboard, &shutdown_config).await;
    } else {
        tracing::debug!("SAGE_STOP_MODELS_ON_SHUTDOWN is disabled; leaving default models running");
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use clients::switchboard::VllmInstance;
    use config::{DefaultModel, SageConfig};
    use std::collections::HashMap;

    fn model(name: &str) -> DefaultModel {
        DefaultModel {
            name: name.to_string(),
            gpu_memory_utilization: None,
            max_model_len: None,
            quantization: None,
            dtype: None,
            limit_mm_per_prompt: None,
            enable_tool_calling: false,
            task: None,
        }
    }

    fn instance(model: &str, status: &str) -> VllmInstance {
        VllmInstance {
            id: format!("pid-{model}"),
            namespace: "native".to_string(),
            model: model.to_string(),
            host: "0.0.0.0".to_string(),
            port: 8000,
            quantization: None,
            max_model_len: None,
            gpu_memory_utilization: None,
            enable_prefix_caching: false,
            task: None,
            started_at: chrono::Utc::now(),
            status: status.to_string(),
        }
    }

    fn test_config(models: &[&str]) -> SageConfig {
        SageConfig {
            system_prompt: String::new(),
            default_models: models.iter().map(|n| model(n)).collect(),
            supported_models: vec!["*".to_string()],
            default_search_provider: "none".to_string(),
            available_search_providers: vec![],
            capability_profile: tools::capabilities::get_profile("web_assistant").unwrap(),
            stop_models_on_shutdown: false,
        }
    }

    #[test]
    fn launches_the_first_missing_model() {
        let config = test_config(&["chat", "embed"]);
        let next = next_model_to_launch(&config, &[], &HashMap::new()).unwrap();
        assert_eq!(next.name, "chat");
    }

    #[test]
    fn waits_while_another_model_is_starting() {
        let config = test_config(&["chat", "embed"]);
        let instances = vec![instance("chat", "starting")];
        assert!(next_model_to_launch(&config, &instances, &HashMap::new()).is_none());

        let instances = vec![instance("chat", "pending")];
        assert!(next_model_to_launch(&config, &instances, &HashMap::new()).is_none());
    }

    #[test]
    fn moves_on_once_the_previous_model_is_running() {
        let config = test_config(&["chat", "embed"]);
        let instances = vec![instance("chat", "running")];
        let next = next_model_to_launch(&config, &instances, &HashMap::new()).unwrap();
        assert_eq!(next.name, "embed");
    }

    #[test]
    fn relaunches_a_failed_model_before_the_rest() {
        let config = test_config(&["chat", "embed"]);
        let instances = vec![instance("chat", "failed")];
        let next = next_model_to_launch(&config, &instances, &HashMap::new()).unwrap();
        assert_eq!(next.name, "chat");
    }

    #[test]
    fn skips_a_model_that_exhausted_its_attempts() {
        let config = test_config(&["chat", "embed"]);
        let instances = vec![instance("chat", "failed")];
        let attempts = HashMap::from([("chat".to_string(), MAX_LAUNCH_ATTEMPTS)]);
        let next = next_model_to_launch(&config, &instances, &attempts).unwrap();
        assert_eq!(next.name, "embed");
    }

    #[test]
    fn skips_unsupported_models() {
        let mut config = test_config(&["chat", "embed"]);
        config.supported_models = vec!["embed".to_string()];
        let next = next_model_to_launch(&config, &[], &HashMap::new()).unwrap();
        assert_eq!(next.name, "embed");
    }

    #[test]
    fn nothing_to_launch_when_all_models_are_running() {
        let config = test_config(&["chat", "embed"]);
        let instances = vec![instance("chat", "running"), instance("embed", "running")];
        assert!(next_model_to_launch(&config, &instances, &HashMap::new()).is_none());
    }
}
