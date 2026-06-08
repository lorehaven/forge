use crate::clients::switchboard::{SwitchboardClient, VllmInstance};
use crate::models::Conversation;
use crate::routers::ui::common::{UiPageKind, render_page};
use actix_web::{HttpResponse, Responder, get, web};
use quench_db::prelude::{Crud, Db};
use quench_srv::actix::routers::ui::pages::home::handle_home;
use quench_srv::prelude::{JwtConfig, with_base_path};
use quench_web::prelude::*;

#[derive(serde::Deserialize)]
pub struct HomeQuery {
    pub conversation_id: Option<String>,
}

async fn handle_home_page(
    req: actix_web::HttpRequest,
    config: web::Data<JwtConfig>,
    switchboard: web::Data<SwitchboardClient>,
    db: web::Data<Db>,
    chat_state: web::Data<crate::routers::ui::chat::ChatState>,
    query: web::Query<HomeQuery>,
) -> impl Responder {
    let instances = switchboard.get_vllm_instances().await;

    let repo = db.repository::<Conversation>();
    let mut conversations = repo.list().await.unwrap_or_default();
    conversations.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

    let active_id = query
        .conversation_id
        .clone()
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let mut active_messages = Vec::new();
    let mut active_message_id = None;
    if let Some(ref cid) = query.conversation_id {
        if let Ok(Some(conv)) = repo.read(cid).await {
            active_message_id = conv.active_message_id;
        }
    }

    if let Some(ref amid) = active_message_id {
        if let Ok(nodes) = crate::routers::ui::chat::get_conversation_message_nodes(&db, Some(amid)).await {
            for node in nodes {
                let siblings = crate::routers::ui::chat::get_siblings(&db, &node.conversation_id, node.parent_id.as_deref()).await.unwrap_or_default();
                active_messages.push((node, siblings));
            }
        }
    }

    // AUTO-TRIGGER LOGIC:
    // If the last message is from a user, we need to auto-trigger an AI response.
    let mut auto_trigger_ai = None;
    if let Some((last_msg, _)) = active_messages.last() {
        if last_msg.role == "user" {
            // Find an available model instance
            if let Ok(ref insts) = instances {
                if let Some(instance) = insts.first() {
                    let pending_id = uuid::Uuid::new_v4().to_string();
                    let chat_req = crate::routers::ui::chat::ChatRequest {
                        instance_id: instance.id.clone(),
                        message: last_msg.content.clone(),
                        conversation_id: last_msg.conversation_id.clone(),
                        parent_id: Some(last_msg.id.clone()),
                        skip_user_message: true, // DB message already exists
                    };
                    chat_state.pending_messages.insert(pending_id.clone(), chat_req);
                    auto_trigger_ai = Some(pending_id);
                }
            }
        }
    }

    handle_home(req, config, move || {
        render_home_page(instances, conversations, active_id, active_messages, auto_trigger_ai)
    })
    .await
}

#[get("/home")]
pub(super) async fn home(
    req: actix_web::HttpRequest,
    config: web::Data<JwtConfig>,
    switchboard: web::Data<SwitchboardClient>,
    db: web::Data<Db>,
    chat_state: web::Data<crate::routers::ui::chat::ChatState>,
    query: web::Query<HomeQuery>,
) -> impl Responder {
    handle_home_page(req, config, switchboard, db, chat_state, query).await
}

#[get("/home/")]
pub(super) async fn home_slash(
    req: actix_web::HttpRequest,
    config: web::Data<JwtConfig>,
    switchboard: web::Data<SwitchboardClient>,
    db: web::Data<Db>,
    chat_state: web::Data<crate::routers::ui::chat::ChatState>,
    query: web::Query<HomeQuery>,
) -> impl Responder {
    handle_home_page(req, config, switchboard, db, chat_state, query).await
}

fn render_home_page(
    instances_res: anyhow::Result<Vec<VllmInstance>>,
    conversations: Vec<Conversation>,
    active_id: String,
    active_messages: Vec<(crate::models::Message, Vec<crate::models::Message>)>,
    auto_trigger_ai: Option<String>,
) -> HttpResponse {
    let mut model_select = select().class("model-selector").attr("id", "model-select");

    let has_model = match &instances_res {
        Ok(instances) => !instances.is_empty(),
        Err(_) => false,
    };

    match &instances_res {
        Ok(instances) => {
            if instances.is_empty() {
                model_select = model_select.child(
                    option()
                        .attr("value", "")
                        .attr("disabled", "disabled")
                        .attr("selected", "selected")
                        .text("No models available"),
                );
            } else {
                for instance in instances {
                    model_select = model_select
                        .child(option().attr("value", &instance.id).text(&instance.model));
                }
            }
        }
        Err(err) => {
            tracing::error!("Failed to fetch models from switchboard: {}", err);
            model_select = model_select.child(
                option()
                    .attr("value", "")
                    .attr("disabled", "disabled")
                    .attr("selected", "selected")
                    .text("Switchboard unavailable"),
            );
        }
    }

    let mut chat_textarea = textarea()
        .attr("id", "chat-input")
        .attr("name", "message")
        .class("chat-input")
        .attr("rows", "1")
        .attr("data-i18n-placeholder", "ui_chat_input_placeholder")
        .attr("onkeydown", "if(event.key === 'Enter' && !event.shiftKey) { event.preventDefault(); this.form.dispatchEvent(new Event('submit', {cancelable: true, bubbles: true})); }")
        .attr("hx-on:input", "this.style.height = 'auto'; this.style.height = (this.scrollHeight) + 'px';");

    let mut send_btn = button()
        .attr("type", "submit")
        .class("chat-send-btn")
        .child(i().class("fas fa-arrow-up"));

    if !has_model {
        chat_textarea = chat_textarea.attr("disabled", "disabled");
        send_btn = send_btn.attr("disabled", "disabled");
    }

    let mut input_area_container = div().class("chat-input-area-container");
    if !has_model {
        input_area_container = input_area_container.class("disabled");
    }

    input_area_container = input_area_container
        .child_opt((!has_model).then(|| {
            div()
                .class("no-model-warning")
                .child(i().class("fas fa-exclamation-triangle"))
                .child(
                    span()
                        .attr("data-i18n", "ui_chat_no_model_available")
                        .text("No model is currently selected or available."),
                )
        }))
        .child(
            div()
                .class("chat-input-area")
                .child(chat_textarea)
                .child(send_btn),
        )
        .child(
            div()
                .class("chat-input-extras")
                .child(div().attr("style", "flex: 1;"))
                .child(model_select.attr("name", "instance_id")),
        );

    // Sidebar
    let sidebar_header = div().class("sidebar-header").child(
        a().class("new-chat-btn")
            .attr("href", with_base_path("/ui/home"))
            .child(i().class("fas fa-plus"))
            .child(span().text("New Chat")),
    );

    let mut history_list = div().class("history-list").attr("id", "history-list");

    for conv in &conversations {
        let is_active = conv.id == active_id;
        let item_class = if is_active {
            "history-item active"
        } else {
            "history-item"
        };
        let link_class = if is_active {
            "history-item-link active"
        } else {
            "history-item-link"
        };
        let item_id = format!("history-item-{}", conv.id);

        let item = div()
            .class(item_class)
            .attr("id", &item_id)
            .child(
                a().class(link_class)
                    .attr(
                        "href",
                        with_base_path(&format!("/ui/home?conversation_id={}", conv.id)),
                    )
                    .text(&conv.title),
            )
            .child(
                div()
                    .class("menu-container")
                    .child(
                        button()
                            .class("menu-trigger-btn")
                            .child(i().class("fas fa-ellipsis-v")),
                    )
                    .child(
                        div().class("dropdown-menu").child(
                            button()
                                .class("dropdown-item delete-item")
                                .attr(
                                    "hx-get",
                                    with_base_path(&format!(
                                        "/ui/chat/conversations/delete-modal/{}?active_id={}",
                                        conv.id, active_id
                                    )),
                                )
                                .attr("hx-target", "#confirm-delete-modal")
                                .attr("hx-swap", "outerHTML")
                                .child(i().class("fas fa-trash"))
                                .child(span().text("Delete")),
                        ),
                    ),
            );
        history_list = history_list.child(item);
    }

    let sidebar = div()
        .class("history-sidebar")
        .child(sidebar_header)
        .child(history_list);

    // Chat History
    let mut history_div = div()
        .class("chat-history")
        .attr(
            "hx-on:htmx:sse-message",
            "this.scrollTop = this.scrollHeight;",
        )
        .child(div().class("chat-history-spacer"));

    let mut nav_div = div().class("chat-navigation");

    if active_messages.is_empty() {
        history_div = history_div.child(
            div()
                .class("chat-message message-ai")
                .attr("id", "msg-0")
                .child(
                    div().class("message-inner").child(
                        div()
                            .class("message-content")
                            .attr("data-i18n", "ui_chat_welcome_message"),
                    ),
                ),
        );

        nav_div = nav_div.child(
            div()
                .class("nav-dot active")
                .attr("data-msg-id", "msg-0")
                .attr("onclick", "const target = document.getElementById(this.dataset.msgId); if (target) { target.scrollIntoView({behavior: 'smooth', block: 'start'}); }")
                .child(
                    div()
                        .class("nav-tooltip")
                        .text("Hello! I am Sage...")
                )
        );
    } else {
        use crate::routers::ui::common::format::format_message;
        let last_user_idx = active_messages.iter().rposition(|(m, _)| m.role == "user");

        for (idx, (msg, siblings)) in active_messages.iter().enumerate() {
            let role_class = if msg.role == "user" {
                "message-user"
            } else {
                "message-ai"
            };
            let element_id = if msg.role == "user" {
                format!("user-{}", msg.id)
            } else {
                format!("ai-{}", msg.id)
            };
            let trimmed_content = msg.content.trim();

            let total_siblings = siblings.len();
            let sibling_index = siblings.iter().position(|s| s.id == msg.id).unwrap_or(0);
            
            let is_last = idx == active_messages.len() - 1;
            let is_last_user = Some(idx) == last_user_idx;

            let regenerate_btn_opt = (msg.role == "assistant" && is_last).then(|| {
                button()
                    .class("branch-btn regenerate-btn")
                    .attr("hx-post", with_base_path("/ui/chat/regenerate"))
                    .attr("hx-vals", format!(r#"{{"message_id": "{}"}}"#, msg.id))
                    .attr("hx-target", ".chat-history")
                    .attr("hx-swap", "beforeend")
                    .child(i().class("fas fa-sync-alt"))
                    .child(span().text(" Regenerate"))
            });

            let edit_btn_opt = (msg.role == "user" && is_last_user).then(|| {
                button()
                    .class("branch-btn edit-btn")
                    .attr("hx-get", with_base_path(&format!("/ui/chat/edit-form/{}", msg.id)))
                    .attr("hx-target", format!("#user-{}", msg.id))
                    .attr("hx-swap", "innerHTML")
                    .child(i().class("fas fa-edit"))
                    .child(span().text(" Edit"))
            });

            let branch_widget_opt = (total_siblings > 1 || regenerate_btn_opt.is_some() || edit_btn_opt.is_some()).then(|| {
                let mut controls = div().class("branch-controls");
                
                // ONLY SHOW NAVIGATION FOR ASSISTANT MESSAGES
                if total_siblings > 1 && msg.role == "assistant" {
                    let prev_index = if sibling_index == 0 { total_siblings - 1 } else { sibling_index - 1 };
                    let next_index = if sibling_index == total_siblings - 1 { 0 } else { sibling_index + 1 };
                    let prev_sibling = &siblings[prev_index];
                    let next_sibling = &siblings[next_index];

                    let nav = div()
                        .class("branch-nav")
                        .child(
                            form()
                                .attr("hx-post", with_base_path("/ui/chat/conversations/switch"))
                                .attr("style", "display: inline;")
                                .child(input().attr("type", "hidden").attr("name", "conversation_id").attr("value", &msg.conversation_id))
                                .child(input().attr("type", "hidden").attr("name", "target_message_id").attr("value", &prev_sibling.id))
                                .child(
                                    button()
                                        .class("branch-btn")
                                        .attr("type", "submit")
                                        .child(i().class("fas fa-chevron-left"))
                                )
                        )
                        .child(
                            span()
                                .class("branch-info")
                                .text(format!("{}/{}", sibling_index + 1, total_siblings))
                        )
                        .child(
                            form()
                                .attr("hx-post", with_base_path("/ui/chat/conversations/switch"))
                                .attr("style", "display: inline;")
                                .child(input().attr("type", "hidden").attr("name", "conversation_id").attr("value", &msg.conversation_id))
                                .child(input().attr("type", "hidden").attr("name", "target_message_id").attr("value", &next_sibling.id))
                                .child(
                                    button()
                                        .class("branch-btn")
                                        .attr("type", "submit")
                                        .child(i().class("fas fa-chevron-right"))
                                )
                        );
                    controls = controls.child(nav);
                }

                if let Some(btn) = regenerate_btn_opt {
                    controls = controls.child(btn);
                }

                if let Some(btn) = edit_btn_opt {
                    controls = controls.child(btn);
                }
                
                controls
            });

            let chat_msg = div()
                .class(format!("chat-message {}", role_class))
                .attr("id", &element_id)
                .child(
                    div()
                        .class("message-inner")
                        .raw()
                        .text(format_message(trimmed_content))
                        .child_opt(branch_widget_opt),
                );
            history_div = history_div.child(chat_msg);

            let preview_raw: String = trimmed_content.chars().take(30).collect();
            let preview = if trimmed_content.chars().count() > 30 {
                format!("{}...", preview_raw)
            } else {
                preview_raw
            };

            let dot = div()
                .class("nav-dot")
                .attr("data-msg-id", &element_id)
                .attr("onclick", "const target = document.getElementById(this.dataset.msgId); if (target) { target.scrollIntoView({behavior: 'smooth', block: 'start'}); }")
                .child(
                    div()
                        .class("nav-tooltip")
                        .text(preview)
                );
            nav_div = nav_div.child(dot);
        }

        // If we are auto-triggering AI (e.g. after an edit or branching)
        if let Some(pending_id) = auto_trigger_ai {
            let stream_url = with_base_path(&format!("/ui/chat/stream/{}", pending_id));
            let ai_thinking_msg = div()
                .class("chat-message message-ai")
                .attr("id", format!("ai-{}", pending_id))
                .attr("hx-ext", "sse")
                .attr("sse-connect", stream_url)
                .attr("sse-swap", "message")
                .child(
                    div()
                        .class("message-inner")
                        .child(div().class("message-content").text("Sage is thinking...")),
                );
            history_div = history_div.child(ai_thinking_msg);

            let ai_dot = div()
                .class("nav-dot active")
                .attr("id", format!("dot-ai-{}", pending_id))
                .attr("data-msg-id", format!("ai-{}", pending_id))
                .attr("onclick", "const target = document.getElementById(this.dataset.msgId); if (target) { target.scrollIntoView({behavior: 'smooth', block: 'start'}); }")
                .child(
                    div()
                        .class("nav-tooltip")
                        .attr("id", format!("tooltip-ai-{}", pending_id))
                        .text("Sage is thinking..."),
                );
            nav_div = nav_div.child(ai_dot);
        }
    }

    render_page(
        HttpResponse::Ok(),
        content().class("home-content").child(
            div()
                .attr("style", "display: flex; flex-direction: row; flex: 1; height: 100%; width: 100%; overflow: hidden;")
                .child(sidebar)
                .child(
                    div()
                        .class("chat-container")
                        .child(history_div)
                        .child(nav_div)
                        .child(
                            form()
                                .attr("hx-post", with_base_path("/ui/chat/send"))
                                .attr("hx-target", ".chat-history")
                                .attr("hx-swap", "beforeend")
                                .attr("hx-on::after-request", "if(event.detail.successful) { document.getElementById('chat-input').value = ''; document.getElementById('chat-input').style.height = 'auto'; const history = document.querySelector('.chat-history'); history.scrollTop = history.scrollHeight; }")
                                .class("chat-input-wrapper")
                                .child(input().attr("type", "hidden").attr("name", "conversation_id").attr("value", &active_id))
                                .child(input_area_container)
                        )
                )
        )
        .child(div().attr("id", "confirm-delete-modal").class("estimates-modal"))
        .child(
            script(r#"
                (function() {
                    function scrollToBottom() {
                        const history = document.querySelector('.chat-history');
                        if (history) {
                            requestAnimationFrame(() => {
                                history.scrollTop = history.scrollHeight;
                            });
                        }
                    }

                    function updateActiveDot() {
                        const historyContainer = document.querySelector('.chat-history');
                        if (!historyContainer) return;

                        const messages = document.querySelectorAll('.chat-message');
                        const dots = document.querySelectorAll('.nav-dot');
                        if (messages.length === 0 || dots.length === 0) return;

                        let activeIndex = 0;
                        const containerRect = historyContainer.getBoundingClientRect();
                        const threshold = containerRect.top + (containerRect.height / 3);

                        const atBottom = Math.abs(historyContainer.scrollHeight - historyContainer.scrollTop - historyContainer.clientHeight) < 100;
                        
                        if (atBottom) {
                            activeIndex = messages.length - 1;
                        } else {
                            for (let i = 0; i < messages.length; i++) {
                                const rect = messages[i].getBoundingClientRect();
                                if (rect.top < threshold) {
                                    activeIndex = i;
                                } else {
                                    break;
                                }
                            }
                        }

                        dots.forEach((dot, i) => {
                            if (i === activeIndex) {
                                dot.classList.add('active');
                            } else {
                                dot.classList.remove('active');
                            }
                        });
                    }

                    document.addEventListener('scroll', (e) => {
                        if (e.target.classList && e.target.classList.contains('chat-history')) {
                            updateActiveDot();
                        }
                    }, true);

                    document.addEventListener('htmx:afterSwap', (e) => {
                        updateActiveDot();
                        if (e.detail && e.detail.target && e.detail.target.classList && e.detail.target.classList.contains('chat-history')) {
                            scrollToBottom();
                        }
                    });

                    document.addEventListener('htmx:oobAfterSwap', (e) => {
                        scrollToBottom();
                        updateActiveDot();
                    });

                    // Update on SSE messages too
                    document.addEventListener('htmx:sseMessage', (e) => {
                        scrollToBottom();
                    });

                    setTimeout(() => {
                        scrollToBottom();
                        updateActiveDot();
                    }, 100);
                })();
            "#.to_string()).raw()
        ),
        UiPageKind::Home,
    )
}
