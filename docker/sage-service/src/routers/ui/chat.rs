use crate::clients::switchboard::SwitchboardClient;
use crate::clients::vllm::{ChatMessage, VllmClient};
use crate::config::SageConfig;
use crate::routers::ui::common::format::format_message;
use actix_web::{HttpResponse, Responder, get, post, web};
use dashmap::DashMap;
use futures_util::StreamExt;
use quench_srv::prelude::with_base_path;
use quench_web::prelude::*;
use serde::Deserialize;
use uuid::Uuid;

pub struct ChatState {
    pub pending_messages: DashMap<String, ChatRequest>,
}

#[derive(Deserialize, Clone)]
pub struct ChatRequest {
    pub instance_id: String,
    pub message: String,
}

#[post("/send")]
pub async fn send_message(
    form: web::Form<ChatRequest>,
    state: web::Data<ChatState>,
) -> impl Responder {
    let message_id = Uuid::new_v4().to_string();
    let req = form.into_inner();

    let user_preview: String = req.message.chars().take(30).collect();
    let user_preview = if req.message.chars().count() > 30 {
        format!("{}...", user_preview)
    } else {
        user_preview
    };

    state
        .pending_messages
        .insert(message_id.clone(), req.clone());

    let user_msg = div()
        .class("chat-message message-user")
        .attr("id", format!("user-{}", message_id))
        .child(
            div().class("message-inner").child(
                div()
                    .class("message-content")
                    .raw()
                    .text(format_message(&req.message)),
            ),
        );

    let stream_url = with_base_path(&format!("/ui/chat/stream/{}", message_id));

    let ai_msg = div()
        .class("chat-message message-ai")
        .attr("id", format!("ai-{}", message_id))
        .attr("hx-ext", "sse")
        .attr("sse-connect", stream_url)
        .attr("sse-swap", "message")
        .child(
            div()
                .class("message-inner")
                .child(div().class("message-content").text("Sage is thinking...")),
        );

    let user_dot = div()
        .attr("hx-swap-oob", "beforeend:.chat-navigation")
        .child(
            div()
                .class("nav-dot")
                .attr("data-msg-id", format!("user-{}", message_id))
                .child(div().class("nav-tooltip").text(user_preview)),
        );

    let ai_dot = div()
        .attr("hx-swap-oob", "beforeend:.chat-navigation")
        .child(
            div()
                .class("nav-dot")
                .attr("id", format!("dot-ai-{}", message_id))
                .attr("data-msg-id", format!("ai-{}", message_id))
                .child(
                    div()
                        .class("nav-tooltip")
                        .attr("id", format!("tooltip-ai-{}", message_id))
                        .text("Sage is thinking..."),
                ),
        );

    HttpResponse::Ok().content_type("text/html").body(format!(
        "{}{}{}{}",
        user_msg.render(),
        ai_msg.render(),
        user_dot.render(),
        ai_dot.render()
    ))
}

fn encode_sse(event: &str, data: &str) -> actix_web::web::Bytes {
    let mut sse = format!("event: {}\n", event);
    for line in data.split('\n') {
        sse.push_str("data: ");
        sse.push_str(line);
        sse.push('\n');
    }
    sse.push('\n');
    actix_web::web::Bytes::from(sse)
}

#[get("/stream/{id}")]
pub async fn stream_message(
    id: web::Path<String>,
    state: web::Data<ChatState>,
    switchboard: web::Data<SwitchboardClient>,
    vllm: web::Data<VllmClient>,
    config: web::Data<SageConfig>,
) -> impl Responder {
    let message_id = id.into_inner();

    let req = match state.pending_messages.get(&message_id) {
        Some(r) => r.clone(),
        None => return HttpResponse::NotFound().finish(),
    };

    let instances = match switchboard.get_vllm_instances().await {
        Ok(i) => i,
        Err(err) => return HttpResponse::InternalServerError().body(err.to_string()),
    };

    let Some(instance) = instances.into_iter().find(|i| i.id == req.instance_id) else {
        return HttpResponse::NotFound().body("Model instance not found");
    };

    let max_tokens = instance.max_model_len.unwrap_or(2048).min(2048); // CAP TO 2048 TOTAL

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
        .chat_stream(
            &instance.host,
            instance.port,
            &instance.model,
            messages,
            Some(max_tokens),
        )
        .await
    {
        Ok(s) => s,
        Err(err) => return HttpResponse::InternalServerError().body(err.to_string()),
    };

    let mut full_content = String::new();
    let message_id_clone = message_id.clone();

    let sse_stream = async_stream::stream! {
        let mut stream = stream;
        while let Some(res) = stream.next().await {
            match res {
                Ok(content) => {
                    full_content.push_str(&content);
                    let rendered = format_message(&full_content);

                    let ai_preview_raw: String = full_content.chars().take(30).collect();
                    let ai_preview = if full_content.chars().count() > 30 {
                        format!("{}...", ai_preview_raw)
                    } else {
                        ai_preview_raw
                    };

                    let html = div()
                        .class("message-inner")
                        .child(div().class("message-content").raw().text(rendered))
                        .render();

                    let tooltip_oob = div()
                        .class("nav-tooltip")
                        .attr("id", format!("tooltip-ai-{}", message_id_clone))
                        .attr("hx-swap-oob", "true")
                        .text(ai_preview)
                        .render();

                    yield Ok::<_, actix_web::Error>(encode_sse("message", &format!("{}{}", html, tooltip_oob)));
                }
                Err(err) => {
                    let html = div()
                        .class("message-inner")
                        .child(div().class("message-content").text(format!("Error: {}", err)))
                        .render();
                    yield Ok::<_, actix_web::Error>(encode_sse("message", &html));
                    return;
                }
            }
        }

        // Final swap to static element to close connection
        let final_rendered = format_message(&full_content);
        let final_msg = div()
            .class("chat-message message-ai")
            .attr("id", format!("ai-{}", message_id_clone))
            .attr("hx-swap-oob", "true")
            .child(
                div()
                    .class("message-inner")
                    .child(div().class("message-content").raw().text(final_rendered))
            );

        yield Ok::<_, actix_web::Error>(encode_sse("message", &final_msg.render()));
        state.pending_messages.remove(&message_id_clone);
    };

    HttpResponse::Ok()
        .content_type("text/event-stream")
        .streaming(sse_stream)
}

pub fn scope() -> actix_web::Scope {
    web::scope("/chat")
        .service(send_message)
        .service(stream_message)
}
