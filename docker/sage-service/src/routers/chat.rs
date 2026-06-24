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

#[post("")]
pub async fn chat(
    req: web::Json<ChatRequest>,
    switchboard: web::Data<SwitchboardClient>,
    vllm: web::Data<VllmClient>,
    config: web::Data<SageConfig>,
) -> impl Responder {
    let instances = match switchboard.get_vllm_instances().await {
        Ok(i) => i,
        Err(err) => return HttpResponse::InternalServerError().body(err.to_string()),
    };

    let Some(instance) = instances.into_iter().find(|i| i.id == req.instance_id) else {
        return HttpResponse::NotFound().body("Model instance not found");
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
        Err(err) => return HttpResponse::InternalServerError().body(err.to_string()),
    };

    let sse_stream = stream.map(|res| match res {
        Ok(content) => {
            let data = serde_json::json!({ "content": content });
            Ok::<_, actix_web::Error>(web::Bytes::from(format!("data: {}\n\n", data)))
        }
        Err(err) => {
            let data = serde_json::json!({ "error": err.to_string() });
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

pub fn scope() -> actix_web::Scope {
    web::scope("/api/v1/chat")
        .service(chat)
        .service(capabilities)
}
