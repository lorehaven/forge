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
    let conv_opt = if let Some(ref cid) = query.conversation_id {
        repo.read(cid).await.ok().flatten()
    } else {
        None
    };
    if let Some(msgs) = conv_opt.and_then(|conv| {
        serde_json::from_str::<Vec<crate::clients::vllm::ChatMessage>>(&conv.messages).ok()
    }) {
        active_messages = msgs;
    }

    handle_home(req, config, move || {
        render_home_page(instances, conversations, active_id, active_messages)
    })
    .await
}

#[get("/home")]
pub(super) async fn home(
    req: actix_web::HttpRequest,
    config: web::Data<JwtConfig>,
    switchboard: web::Data<SwitchboardClient>,
    db: web::Data<Db>,
    query: web::Query<HomeQuery>,
) -> impl Responder {
    handle_home_page(req, config, switchboard, db, query).await
}

#[get("/home/")]
pub(super) async fn home_slash(
    req: actix_web::HttpRequest,
    config: web::Data<JwtConfig>,
    switchboard: web::Data<SwitchboardClient>,
    db: web::Data<Db>,
    query: web::Query<HomeQuery>,
) -> impl Responder {
    handle_home_page(req, config, switchboard, db, query).await
}

fn render_home_page(
    instances_res: anyhow::Result<Vec<VllmInstance>>,
    conversations: Vec<Conversation>,
    active_id: String,
    active_messages: Vec<crate::clients::vllm::ChatMessage>,
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
        for (i, msg) in active_messages.iter().enumerate() {
            let role_class = if msg.role == "user" {
                "message-user"
            } else {
                "message-ai"
            };
            let element_id = format!("{}-{}", msg.role, i);
            let trimmed_content = msg.content.trim();

            let chat_msg = div()
                .class(format!("chat-message {}", role_class))
                .attr("id", &element_id)
                .child(
                    div()
                        .class("message-inner")
                        .raw()
                        .text(format_message(trimmed_content)),
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
                    function updateActiveDot() {
                        const historyContainer = document.querySelector('.chat-history');
                        if (!historyContainer) return;

                        const messages = document.querySelectorAll('.chat-message');
                        const dots = document.querySelectorAll('.nav-dot');
                        if (messages.length === 0 || dots.length === 0) return;

                        let activeIndex = 0;
                        const containerRect = historyContainer.getBoundingClientRect();
                        const threshold = containerRect.top + (containerRect.height / 3);

                        const atBottom = Math.abs(historyContainer.scrollHeight - historyContainer.scrollTop - historyContainer.clientHeight) < 50;
                        
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
                    });

                    setTimeout(() => {
                        const history = document.querySelector('.chat-history');
                        if (history) {
                            history.scrollTop = history.scrollHeight;
                        }
                        updateActiveDot();
                    }, 100);
                })();
            "#.to_string()).raw()
        ),
        UiPageKind::Home,
    )
}
