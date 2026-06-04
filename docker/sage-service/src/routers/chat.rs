use crate::clients::switchboard::SwitchboardClient;
use crate::clients::vllm::{ChatMessage, VllmClient};
use crate::config::SageConfig;
use actix_web::{HttpResponse, Responder, post, web};
use futures_util::StreamExt;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct ChatRequest {
    pub instance_id: String,
    pub message: String,
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
        },
        ChatMessage {
            role: "user".to_string(),
            content: req.message.clone(),
        },
    ];

    let stream = match vllm
        .chat_stream(&instance.host, instance.port, &instance.model, messages)
        .await
    {
        Ok(s) => s,
        Err(err) => return HttpResponse::InternalServerError().body(err.to_string()),
    };

    let sse_stream = stream.map(|res| match res {
        Ok(content) => {
            let data = serde_json::json!({ "content": content });
            Ok::<_, actix_web::Error>(actix_web::web::Bytes::from(format!("data: {}\n\n", data)))
        }
        Err(err) => {
            let data = serde_json::json!({ "error": err.to_string() });
            Ok::<_, actix_web::Error>(actix_web::web::Bytes::from(format!("data: {}\n\n", data)))
        }
    });

    HttpResponse::Ok()
        .content_type("text/event-stream")
        .streaming(sse_stream)
}

pub fn scope() -> actix_web::Scope {
    web::scope("/api/v1/chat").service(chat)
}
