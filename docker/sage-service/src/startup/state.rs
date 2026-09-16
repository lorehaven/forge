use dashmap::DashMap;
use quench_auth::http::domain::sso_client::SsoConfig;
use quench_auth::prelude::{JwtConfig, SessionDb, UserDb};
use quench_starter::prelude::DbWrapper;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::clients::switchboard::SwitchboardClient;
use crate::clients::vllm::VllmClient;
use crate::config::SageConfig;
use crate::observability::cost_tracking::CostTracker;
use crate::observability::metrics::MetricsCollector;
use crate::routers::ui::chat::ChatState;
use crate::runtime::rate_limiter::RateLimiter;
use crate::tools;

/// Everything the HTTP layer needs, built once at startup and provided into
/// the DI container in `main.rs`.
#[derive(Clone)]
pub struct AppState {
    pub switchboard: SwitchboardClient,
    pub vllm: VllmClient,
    pub config: SageConfig,
    pub chat_state: Arc<ChatState>,
    pub jwt_config: JwtConfig,
    pub sso_config: SsoConfig,
    pub user_db: Arc<UserDb>,
    pub session_db: Arc<SessionDb>,
    pub tool_registry: Arc<tools::ToolRegistry>,
    pub search_providers: Arc<tools::SearchProviderRegistry>,
    pub metrics: Arc<MetricsCollector>,
    pub rate_limiter: Arc<Mutex<RateLimiter>>,
    pub cost_tracker: Arc<CostTracker>,
}

impl AppState {
    /// Build every shared value the service needs; the returned `DbWrapper` goes to `serve`, which owns migrations and health reporting.
    pub async fn init() -> (Self, Arc<DbWrapper>) {
        let db_wrapper = DbWrapper::init_env().await;
        let switchboard = SwitchboardClient::new();
        let vllm = VllmClient::new();
        let config = SageConfig::load();
        let search_providers = init_search_providers();
        let tool_registry = init_tool_registry(
            &search_providers,
            &config.capability_profile,
            db_wrapper.db.clone(),
            switchboard.clone(),
            vllm.clone(),
        );

        let state = Self {
            switchboard,
            vllm,
            chat_state: Arc::new(ChatState {
                pending_messages: DashMap::new(),
            }),
            jwt_config: JwtConfig::init().await,
            sso_config: SsoConfig::init(),
            user_db: UserDb::init(db_wrapper.db.clone()).await,
            session_db: SessionDb::from_env().await.expect("session store"),
            tool_registry,
            search_providers,
            metrics: Arc::new(MetricsCollector::new()),
            rate_limiter: Arc::new(Mutex::new(RateLimiter::new())),
            cost_tracker: Arc::new(CostTracker::new()),
            config,
        };

        (state, db_wrapper)
    }
}

fn init_search_providers() -> Arc<tools::SearchProviderRegistry> {
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

    Arc::new(registry)
}

fn init_tool_registry(
    search_providers: &Arc<tools::SearchProviderRegistry>,
    profile: &tools::CapabilityProfile,
    db: quench_db::prelude::Db,
    switchboard: SwitchboardClient,
    vllm: VllmClient,
) -> Arc<tools::ToolRegistry> {
    let mut registry = tools::ToolRegistry::with_profile(profile.clone());

    registry.register(
        "web_search".to_string(),
        Box::new(tools::web_search::WebSearchExecutor::new(
            search_providers.clone(),
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

    // Registered without a conversation context; the chat flow builds a request-scoped registry that carries the actual conversation id.
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

    // Registered without a conversation context; the chat flow builds a request-scoped registry that carries the actual conversation id.
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

    Arc::new(registry)
}
