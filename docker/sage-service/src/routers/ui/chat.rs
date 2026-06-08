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
    pub conversation_id: String,
}

#[post("/send")]
pub async fn send_message(
    form: web::Form<ChatRequest>,
    state: web::Data<ChatState>,
) -> impl Responder {
    let message_id = Uuid::new_v4().to_string();
    let mut req = form.into_inner();
    req.message = req.message.trim().to_string();

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
            div()
                .class("message-inner")
                .raw()
                .text(format_message(&req.message)),
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
                .attr("onclick", "const target = document.getElementById(this.dataset.msgId); if (target) { target.scrollIntoView({behavior: 'smooth', block: 'start'}); }")
                .child(div().class("nav-tooltip").text(user_preview)),
        );

    let ai_dot = div()
        .attr("hx-swap-oob", "beforeend:.chat-navigation")
        .child(
            div()
                .class("nav-dot")
                .attr("id", format!("dot-ai-{}", message_id))
                .attr("data-msg-id", format!("ai-{}", message_id))
                .attr("onclick", "const target = document.getElementById(this.dataset.msgId); if (target) { target.scrollIntoView({behavior: 'smooth', block: 'start'}); }")
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
    db: web::Data<quench_db::prelude::Db>,
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

    let max_model_len = instance.max_model_len.unwrap_or(2048) as usize;
    let reserved_for_generation = if max_model_len > 4096 {
        2048
    } else if max_model_len > 2048 {
        1024
    } else {
        512
    };

    let prompt_budget = max_model_len.saturating_sub(reserved_for_generation);

    fn estimate_tokens(msg: &ChatMessage) -> usize {
        msg.content.chars().count().div_ceil(3) + 4
    }

    let system_message = ChatMessage {
        role: "system".to_string(),
        content: config.system_prompt.clone(),
    };

    let current_user_message = ChatMessage {
        role: "user".to_string(),
        content: req.message.clone(),
    };

    let system_tokens = estimate_tokens(&system_message);
    let current_user_tokens = estimate_tokens(&current_user_message);

    use quench_db::prelude::Crud;
    let repo = db.repository::<crate::models::Conversation>();

    let mut history_messages = Vec::new();
    if let Some(existing_msgs) = repo
        .read(&req.conversation_id)
        .await
        .ok()
        .flatten()
        .and_then(|conv| serde_json::from_str::<Vec<ChatMessage>>(&conv.messages).ok())
    {
        history_messages = existing_msgs;
    }

    let mut selected_history = std::collections::VecDeque::new();
    let mut current_budget_used = system_tokens + current_user_tokens;

    for msg in history_messages.into_iter().rev() {
        let msg_tokens = estimate_tokens(&msg);
        if current_budget_used + msg_tokens <= prompt_budget {
            current_budget_used += msg_tokens;
            selected_history.push_front(msg);
        } else {
            break;
        }
    }

    let mut messages = vec![system_message];
    messages.extend(selected_history);
    messages.push(current_user_message);

    let max_tokens = reserved_for_generation as u32;

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
    let db_clone = db.clone();

    let sse_stream = async_stream::stream! {
        let mut stream = stream;
        while let Some(res) = stream.next().await {
            match res {
                Ok(content) => {
                    full_content.push_str(&content);
                    let trimmed_content = full_content.trim();
                    let rendered = format_message(trimmed_content);

                    let ai_preview_raw: String = trimmed_content.chars().take(30).collect();
                    let ai_preview = if trimmed_content.chars().count() > 30 {
                        format!("{}...", ai_preview_raw)
                    } else {
                        ai_preview_raw
                    };

                    let html = div()
                        .class("message-inner")
                        .raw()
                        .text(rendered)
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
        let final_rendered = format_message(full_content.trim());
        let final_msg = div()
            .class("chat-message message-ai")
            .attr("id", format!("ai-{}", message_id_clone))
            .attr("hx-swap-oob", "true")
            .child(
                div()
                    .class("message-inner")
                    .raw()
                    .text(final_rendered)
            );

        // Update database conversation history
        let repo = db_clone.repository::<crate::models::Conversation>();
        let mut conv_messages = Vec::new();

        if let Some(existing_msgs) = repo.read(&req.conversation_id).await.ok().flatten().and_then(|conv| {
            serde_json::from_str::<Vec<ChatMessage>>(&conv.messages).ok()
        }) {
            conv_messages = existing_msgs;
        }

        conv_messages.push(ChatMessage {
            role: "user".to_string(),
            content: req.message.trim().to_string(),
        });
        conv_messages.push(ChatMessage {
            role: "assistant".to_string(),
            content: full_content.trim().to_string(),
        });

        let mut oob_history_list = String::new();
        if let Ok(msgs_str) = serde_json::to_string(&conv_messages) {
            let updated_at = chrono::Utc::now().to_rfc3339();

            let mut title = if conv_messages.len() <= 2 {
                if req.message.chars().count() > 30 {
                    format!("{}...", req.message.chars().take(30).collect::<String>())
                } else {
                    req.message.clone()
                }
            } else {
                "New Conversation".to_string()
            };

            if let Ok(Some(existing_conv)) = repo.read(&req.conversation_id).await {
                title = existing_conv.title;
            }

            let conv = crate::models::Conversation {
                id: req.conversation_id.clone(),
                title,
                messages: msgs_str,
                updated_at,
            };

            let exists = repo.read(&req.conversation_id).await.map(|o| o.is_some()).unwrap_or(false);
            if exists {
                let _ = repo.update(&conv).await;
            } else {
                let _ = repo.create(&conv).await;
            }

            // Generate updated history list for OOB swap
            if let Ok(mut conversations) = repo.list().await {
                conversations.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

                let mut history_list = div()
                    .class("history-list")
                    .attr("id", "history-list")
                    .attr("hx-swap-oob", "true");

                for conv_item in &conversations {
                    let is_active = conv_item.id == req.conversation_id;
                    let item_class = if is_active { "history-item active" } else { "history-item" };
                    let link_class = if is_active { "history-item-link active" } else { "history-item-link" };
                    let item_id = format!("history-item-{}", conv_item.id);

                    let item = div()
                        .class(item_class)
                        .attr("id", &item_id)
                        .child(
                            a()
                                .class(link_class)
                                .attr("href", with_base_path(&format!("/ui/home?conversation_id={}", conv_item.id)))
                                .text(&conv_item.title)
                        )
                        .child(
                            div()
                                .class("menu-container")
                                .child(
                                    button()
                                        .class("menu-trigger-btn")
                                        .child(i().class("fas fa-ellipsis-v"))
                                )
                                .child(
                                    div()
                                        .class("dropdown-menu")
                                        .child(
                                            button()
                                                .class("dropdown-item delete-item")
                                                .attr("hx-get", with_base_path(&format!("/ui/chat/conversations/delete-modal/{}?active_id={}", conv_item.id, req.conversation_id)))
                                                .attr("hx-target", "#confirm-delete-modal")
                                                .attr("hx-swap", "outerHTML")
                                                .child(i().class("fas fa-trash"))
                                                .child(span().text("Delete"))
                                        )
                                )
                        );
                    history_list = history_list.child(item);
                }
                oob_history_list = history_list.render();
            }
        }

        let combined_msg = format!("{}{}", final_msg.render(), oob_history_list);
        yield Ok::<_, actix_web::Error>(encode_sse("message", &combined_msg));
        state.pending_messages.remove(&message_id_clone);
    };

    HttpResponse::Ok()
        .content_type("text/event-stream")
        .streaming(sse_stream)
}

#[derive(serde::Deserialize)]
pub struct DeleteQuery {
    pub active_id: Option<String>,
}

#[derive(serde::Deserialize)]
pub struct DeleteModalQuery {
    pub active_id: Option<String>,
}

#[get("/conversations/delete-modal/empty")]
pub async fn delete_modal_empty() -> impl Responder {
    let empty = div()
        .attr("id", "confirm-delete-modal")
        .class("estimates-modal");
    HttpResponse::Ok()
        .content_type("text/html")
        .body(empty.render())
}

#[get("/conversations/delete-modal/{id}")]
pub async fn delete_modal(
    id: web::Path<String>,
    query: web::Query<DeleteModalQuery>,
    db: web::Data<quench_db::prelude::Db>,
) -> impl Responder {
    let conv_id = id.into_inner();
    use quench_db::prelude::Crud;
    let repo = db.repository::<crate::models::Conversation>();

    let mut title = "this conversation".to_string();
    if let Ok(Some(conv)) = repo.read(&conv_id).await {
        title = format!("\"{}\"", conv.title);
    }

    let active_id_val = query.active_id.clone().unwrap_or_default();

    let modal = div()
        .attr("id", "confirm-delete-modal")
        .class("estimates-modal open")
        .child(
            button()
                .class("estimates-modal-backdrop")
                .attr("type", "button")
                .attr(
                    "hx-get",
                    with_base_path("/ui/chat/conversations/delete-modal/empty"),
                )
                .attr("hx-target", "#confirm-delete-modal")
                .attr("hx-swap", "outerHTML"),
        )
        .child(
            div()
                .class("estimates-modal-content small")
                .child(
                    div()
                        .class("estimates-modal-header")
                        .child(div().class("estimates-modal-title").text("Confirm Delete"))
                        .child(
                            button()
                                .class("estimates-modal-close")
                                .attr("type", "button")
                                .attr(
                                    "hx-get",
                                    with_base_path("/ui/chat/conversations/delete-modal/empty"),
                                )
                                .attr("hx-target", "#confirm-delete-modal")
                                .attr("hx-swap", "outerHTML")
                                .child(i().class("fas fa-times")),
                        ),
                )
                .child(
                    div()
                        .class("estimates-modal-body")
                        .child(p().text("Are you sure you want to delete this conversation?"))
                        .child(div().class("model-to-delete-name").text(title))
                        .child(
                            form()
                                .class("confirm-actions")
                                .attr(
                                    "hx-post",
                                    with_base_path(&format!(
                                        "/ui/chat/conversations/delete/{}?active_id={}",
                                        conv_id, active_id_val
                                    )),
                                )
                                .attr("hx-target", "#confirm-delete-modal")
                                .attr("hx-swap", "outerHTML")
                                .child(
                                    button()
                                        .class("button cancel")
                                        .attr("type", "button")
                                        .attr(
                                            "hx-get",
                                            with_base_path(
                                                "/ui/chat/conversations/delete-modal/empty",
                                            ),
                                        )
                                        .attr("hx-target", "#confirm-delete-modal")
                                        .attr("hx-swap", "outerHTML")
                                        .text("Cancel"),
                                )
                                .child(
                                    button()
                                        .class("button danger")
                                        .attr("type", "submit")
                                        .text("Delete"),
                                ),
                        ),
                ),
        );

    HttpResponse::Ok()
        .content_type("text/html")
        .body(modal.render())
}

#[post("/conversations/delete/{id}")]
pub async fn delete_conversation(
    id: web::Path<String>,
    query: web::Query<DeleteQuery>,
    db: web::Data<quench_db::prelude::Db>,
) -> impl Responder {
    let id_str = id.into_inner();
    use quench_db::prelude::Crud;
    let repo = db.repository::<crate::models::Conversation>();
    let _ = repo.delete(&id_str).await;

    let mut response = HttpResponse::Ok();
    if query.active_id.as_deref() == Some(&id_str) {
        response.append_header(("HX-Redirect", with_base_path("/ui/home")));
        return response.body("");
    }

    let close_modal = div()
        .attr("id", "confirm-delete-modal")
        .class("estimates-modal")
        .render();
    let oob_delete = div()
        .attr("id", format!("history-item-{}", id_str))
        .attr("hx-swap-oob", "delete")
        .render();

    response
        .content_type("text/html")
        .body(format!("{}{}", close_modal, oob_delete))
}

pub fn scope() -> actix_web::Scope {
    web::scope("/chat")
        .service(send_message)
        .service(stream_message)
        .service(delete_conversation)
        .service(delete_modal)
        .service(delete_modal_empty)
}
