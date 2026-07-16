use crate::clients::switchboard::SwitchboardClient;
use crate::clients::vllm::{ChatMessage, VllmClient};
use crate::config::SageConfig;
use actix_web::{HttpResponse, Responder, get, post, web};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct ChatRequest {
    pub instance_id: String,
    pub message: String,
}

#[derive(Serialize)]
pub struct CapabilitiesResponse {
    pub profile: String,
    pub description: String,
    pub available_tools: Vec<String>,
    pub available_search_providers: Vec<String>,
}

#[derive(Serialize)]
pub struct MetricsResponse {
    pub profiles: Vec<crate::metrics::ProfileMetrics>,
}

#[derive(Serialize)]
pub struct CostsResponse {
    pub users: Vec<crate::cost_tracking::UserCosts>,
    pub profiles: Vec<crate::cost_tracking::ProfileCosts>,
}

#[post("")]
pub async fn chat(
    req: web::Json<ChatRequest>,
    switchboard: web::Data<SwitchboardClient>,
    vllm: web::Data<VllmClient>,
    config: web::Data<SageConfig>,
) -> impl Responder {
    let instances = match switchboard.get_vllm_instances().await {
        Ok(i) => i,
        Err(err) => {
            tracing::error!("Failed to get vLLM instances from Switchboard: {}", err);
            return HttpResponse::InternalServerError().body("api_error_switchboard_unavailable");
        }
    };

    let Some(instance) = instances.into_iter().find(|i| i.id == req.instance_id) else {
        return HttpResponse::NotFound().body("api_error_instance_not_found");
    };

    let messages = vec![
        ChatMessage {
            role: "system".to_string(),
            content: config.system_prompt.clone(),
            tool_calls: None,
        },
        ChatMessage {
            role: "user".to_string(),
            content: req.message.clone(),
            tool_calls: None,
        },
    ];

    let stream = match vllm
        .chat_stream(
            &instance.host,
            instance.port,
            &instance.model,
            messages,
            instance.max_model_len,
        )
        .await
    {
        Ok(s) => s,
        Err(err) => {
            tracing::error!("Failed to create vLLM chat stream: {}", err);
            return HttpResponse::InternalServerError().body("api_error_stream_failed");
        }
    };

    let sse_stream = stream.map(|res| match res {
        Ok(content) => {
            let data = serde_json::json!({ "content": content });
            Ok::<_, actix_web::Error>(web::Bytes::from(format!("data: {}\n\n", data)))
        }
        Err(err) => {
            // Localization code plus the raw detail for diagnostics.
            let data =
                serde_json::json!({ "error": "api_error_stream_failed", "detail": err.to_string() });
            Ok::<_, actix_web::Error>(web::Bytes::from(format!("data: {}\n\n", data)))
        }
    });

    HttpResponse::Ok()
        .content_type("text/event-stream")
        .streaming(sse_stream)
}

#[get("/capabilities")]
pub async fn capabilities(config: web::Data<SageConfig>) -> impl Responder {
    let profile = &config.capability_profile;
    let mut tools = profile.enabled_tool_names();
    tools.sort();

    let response = CapabilitiesResponse {
        profile: profile.name.clone(),
        description: profile.description.clone(),
        available_tools: tools.into_iter().map(|s| s.to_string()).collect(),
        available_search_providers: config.available_search_providers.clone(),
    };

    HttpResponse::Ok().json(response)
}

#[get("/metrics")]
pub async fn get_metrics(
    metrics_collector: web::Data<std::sync::Arc<crate::metrics::MetricsCollector>>,
) -> impl Responder {
    let profiles = metrics_collector.get_all_profiles_metrics();
    let response = MetricsResponse { profiles };
    HttpResponse::Ok().json(response)
}

#[get("/metrics/{profile}")]
pub async fn get_metrics_by_profile(
    profile_name: web::Path<String>,
    metrics_collector: web::Data<std::sync::Arc<crate::metrics::MetricsCollector>>,
) -> impl Responder {
    let profile = profile_name.into_inner();
    match metrics_collector.get_profile_metrics(&profile) {
        Some(metrics) => HttpResponse::Ok().json(metrics),
        None => HttpResponse::NotFound().body("api_error_metrics_not_found"),
    }
}

#[get("/costs")]
pub async fn get_costs(
    cost_tracker: web::Data<std::sync::Arc<crate::cost_tracking::CostTracker>>,
) -> impl Responder {
    let users = cost_tracker.get_all_user_costs();
    let profiles = cost_tracker.get_all_profile_costs();
    let response = CostsResponse { users, profiles };
    HttpResponse::Ok().json(response)
}

#[get("/costs/user/{user_id}")]
pub async fn get_user_costs(
    user_id: web::Path<String>,
    cost_tracker: web::Data<std::sync::Arc<crate::cost_tracking::CostTracker>>,
) -> impl Responder {
    match cost_tracker.get_user_costs(&user_id) {
        Some(costs) => HttpResponse::Ok().json(costs),
        None => HttpResponse::NotFound().body("api_error_costs_not_found"),
    }
}

#[get("/context-status/{profile}")]
pub async fn get_context_status(profile_path: web::Path<String>) -> impl Responder {
    let profile = profile_path.into_inner();
    let status = crate::context_manager::ContextStatus::new(&profile, 0);
    HttpResponse::Ok().json(status)
}

pub fn scope() -> actix_web::Scope {
    web::scope("/api/v1/chat")
        .service(chat)
        .service(capabilities)
        .service(get_metrics)
        .service(get_metrics_by_profile)
        .service(get_costs)
        .service(get_user_costs)
        .service(get_context_status)
}
