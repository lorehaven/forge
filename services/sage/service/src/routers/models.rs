//! The models: what the composer offers and whether the chat can be used yet (every default model runs).

use crate::clients::switchboard::{SwitchboardClient, VllmInstance};
use crate::config::{DefaultModel, SageConfig};
use crate::routers::files::Username;
use quench_core::uuid::Uuid;
use quench_http::prelude::{Inject, Response, get, http::StatusCode};
use sage_core::{Created, DefaultModelState, ModelOption, ModelsView, SearchProviderOption};

/// Launch state of a configured default model; the chat is blocked until every model reaches `Running`, since all of them are
/// required for Sage to function.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ModelState {
    Running,
    /// Process launched, model still loading weights / warming up.
    Starting,
    /// Not launched yet (monitor task will request a launch shortly).
    Pending,
    Failed,
    /// Switchboard could not be reached, so the real state is unknown.
    Unknown,
}

impl ModelState {
    /// The word the wire and the screens use.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Starting => "starting",
            Self::Pending => "pending",
            Self::Failed => "failed",
            Self::Unknown => "unknown",
        }
    }
}

/// Resolve a model's state from switchboard's instances; if more than one matches (e.g. a stale failed one alongside a fresh
/// starting one), the healthiest state wins.
#[must_use]
pub fn model_state(model: &DefaultModel, instances: &[VllmInstance]) -> ModelState {
    let mut state = ModelState::Pending;
    for inst in instances.iter().filter(|i| i.model == model.name) {
        let candidate = match inst.status.as_str() {
            "running" => ModelState::Running,
            "starting" | "pending" => ModelState::Starting,
            "failed" => ModelState::Failed,
            _ => continue, // ignore "terminating" and anything unexpected
        };
        // Running beats Starting beats Failed beats Pending.
        let rank = |s: ModelState| match s {
            ModelState::Running => 3,
            ModelState::Starting => 2,
            ModelState::Failed => 1,
            _ => 0,
        };
        if rank(candidate) > rank(state) {
            state = candidate;
        }
    }
    state
}

/// True when every configured default model has a running instance.
#[must_use]
pub fn all_models_running(defaults: &[DefaultModel], instances: &[VllmInstance]) -> bool {
    defaults
        .iter()
        .all(|m| model_state(m, instances) == ModelState::Running)
}

fn provider_label(provider: &str) -> &str {
    match provider {
        "brave" => "Brave Search",
        "duckduckgo" => "DuckDuckGo",
        "searxng" => "SearXNG",
        "serpapi" => "SerpAPI",
        other => other,
    }
}

/// The view of the models for a switchboard answer (or its failure).
#[must_use]
pub fn models_view(
    config: &SageConfig,
    instances: &quench_core::Result<Vec<VllmInstance>>,
) -> ModelsView {
    // A model the configuration does not support is never launched, so it is neither waited for nor offered: a default
    // that matches no pattern would hold the chat unready for good, and an instance somebody started by hand in
    // Switchboard is not Sage's to hand out.
    let wanted: Vec<DefaultModel> = config
        .default_models
        .iter()
        .filter(|m| config.is_model_supported(&m.name))
        .cloned()
        .collect();
    let defaults = wanted
        .iter()
        .map(|model| DefaultModelState {
            name: model.name.clone(),
            embedding: matches!(model.task.as_deref(), Some("embed" | "embedding")),
            state: instances
                .as_ref()
                .map_or(ModelState::Unknown, |all| model_state(model, all))
                .word()
                .to_string(),
        })
        .collect();
    ModelsView {
        switchboard_ok: instances.is_ok(),
        ready: instances
            .as_ref()
            .is_ok_and(|all| all_models_running(&wanted, all)),
        // Only chat-capable instances belong in the picker; embedding instances would 404 on chat completions.
        instances: instances.as_ref().map_or_else(
            |_| Vec::new(),
            |all| {
                all.iter()
                    .filter(|i| i.is_chat_capable() && config.is_model_supported(&i.model))
                    .map(|i| ModelOption {
                        id: i.id.clone(),
                        model: i.model.clone(),
                    })
                    .collect()
            },
        ),
        defaults,
        search_providers: config
            .available_search_providers
            .iter()
            .map(|p| SearchProviderOption {
                id: p.clone(),
                label: provider_label(p).to_string(),
            })
            .collect(),
        default_search_provider: config.default_search_provider.clone(),
        accept: crate::routers::files::upload_accept_attribute(),
    }
}

#[get("/api/v1/models")]
pub async fn models(
    username: Username,
    Inject(switchboard): Inject<SwitchboardClient>,
    Inject(config): Inject<SageConfig>,
) -> Response {
    if let Err(response) = username.or_401() {
        return response;
    }
    let instances = switchboard.get_vllm_instances().await;
    if let Err(err) = &instances {
        quench_log::error!("Failed to fetch models from switchboard: {}", err);
    }
    Response::json(StatusCode::OK, &models_view(&config, &instances))
        .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
}

/// An id for a conversation that is not stored yet (the first message or attachment stores it).
#[get("/api/v1/conversation-id")]
pub async fn conversation_id(username: Username) -> Response {
    if let Err(response) = username.or_401() {
        return response;
    }
    Response::json(
        StatusCode::OK,
        &Created {
            id: Uuid::new_v4().to_string(),
        },
    )
    .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
}

pub fn register_routes() {
    let _ = models as fn(_, _, _) -> _;
    let _ = conversation_id as fn(_) -> _;
}
