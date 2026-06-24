use crate::clients::switchboard::{SwitchboardClient, VllmInstance};
use crate::models::Conversation;
use crate::routers::ui::chat::{
    ChatRequest, ChatState, get_conversation_message_nodes, get_siblings,
};
use crate::routers::ui::common;
use crate::routers::ui::common::{UiPageKind, render_page};
use actix_web::{HttpResponse, Responder, get, web};
use quench_db::prelude::{Crud, Db};
use quench_srv::actix::routers::ui::pages::home::handle_home;
use quench_srv::prelude::{JwtConfig, with_base_path};
use quench_web::prelude::*;

#[derive(serde::Deserialize)]
pub struct HomeQuery {
    pub conversation_id: Option<String>,
    pub project_id: Option<String>,
}

async fn handle_home_page(
    req: actix_web::HttpRequest,
    jwt_config: web::Data<JwtConfig>,
    switchboard: web::Data<SwitchboardClient>,
    db: web::Data<Db>,
    chat_state: web::Data<ChatState>,
    sage_config: web::Data<crate::config::SageConfig>,
    query: web::Query<HomeQuery>,
) -> impl Responder {
    let username = match quench_srv::actix::routers::ui::get_user_from_req(&req, &jwt_config).await
    {
        Some(claims) => claims.sub,
        None => return common::ui_login_redirect().map_into_right_body(),
    };

    let instances = switchboard.get_vllm_instances().await;

    // Fetch user's projects
    let project_repo = db.repository::<crate::models::Project>();
    let mut projects = project_repo.list().await.unwrap_or_default();
    projects.retain(|p| p.owner == username);
    projects.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

    let conv_repo = db.repository::<Conversation>();
    let mut conversations = conv_repo.list().await.unwrap_or_default();
    conversations.retain(|c| c.owner == username);
    conversations.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

    let active_id = query
        .conversation_id
        .clone()
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let mut active_messages = Vec::new();
    let mut active_message_id: Option<String> = None;
    if let Some(ref cid) = query.conversation_id
        && let Ok(Some(conv)) = conv_repo.read(cid).await
    {
        active_message_id = conv.active_message_id;
    }

    if let Some(ref amid) = active_message_id
        && let Ok(nodes) = get_conversation_message_nodes(&db, Some(amid)).await
    {
        for node in nodes {
            let siblings = get_siblings(&db, &node.conversation_id, node.parent_id.as_deref())
                .await
                .unwrap_or_default();
            active_messages.push((node, siblings));
        }
    }

    // AUTO-TRIGGER LOGIC:
    // If the last message is from a user, we need to auto-trigger an AI response.
    let mut auto_trigger_ai = None;
    if let Some((last_msg, _)) = active_messages.last()
        && last_msg.role == "user"
        // Find an available model instance
        && let Ok(ref insts) = instances
        && let Some(instance) = insts.first()
    {
        let pending_id = uuid::Uuid::new_v4().to_string();
        let chat_req = ChatRequest {
            instance_id: instance.id.clone(),
            message: last_msg.content.clone(),
            conversation_id: last_msg.conversation_id.clone(),
            project_id: query.project_id.clone(),
            parent_id: Some(last_msg.id.clone()),
            skip_user_message: true, // DB message already exists
            search_provider: None,
            capability_profile: None,
        };
        chat_state
            .pending_messages
            .insert(pending_id.clone(), chat_req);
        auto_trigger_ai = Some(pending_id);
    }

    handle_home(req, jwt_config, move || {
        render_home_page(
            instances,
            projects,
            conversations,
            active_id,
            active_messages,
            auto_trigger_ai,
            query.project_id.clone(),
            sage_config.clone(),
        )
    })
    .await
    .map_into_left_body()
}

#[get("/home")]
pub(super) async fn home(
    req: actix_web::HttpRequest,
    jwt_config: web::Data<JwtConfig>,
    switchboard: web::Data<SwitchboardClient>,
    db: web::Data<Db>,
    chat_state: web::Data<ChatState>,
    sage_config: web::Data<crate::config::SageConfig>,
    query: web::Query<HomeQuery>,
) -> impl Responder {
    handle_home_page(
        req,
        jwt_config,
        switchboard,
        db,
        chat_state,
        sage_config,
        query,
    )
    .await
}

#[get("/home/")]
pub(super) async fn home_slash(
    req: actix_web::HttpRequest,
    jwt_config: web::Data<JwtConfig>,
    switchboard: web::Data<SwitchboardClient>,
    db: web::Data<Db>,
    chat_state: web::Data<ChatState>,
    sage_config: web::Data<crate::config::SageConfig>,
    query: web::Query<HomeQuery>,
) -> impl Responder {
    handle_home_page(
        req,
        jwt_config,
        switchboard,
        db,
        chat_state,
        sage_config,
        query,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
fn render_home_page(
    instances_res: anyhow::Result<Vec<VllmInstance>>,
    projects: Vec<crate::models::Project>,
    conversations: Vec<Conversation>,
    active_id: String,
    active_messages: Vec<(crate::models::Message, Vec<crate::models::Message>)>,
    auto_trigger_ai: Option<String>,
    project_id: Option<String>,
    sage_config: web::Data<crate::config::SageConfig>,
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
        .child({
            // Create provider selector
            let mut provider_select = select()
                .class("provider-selector")
                .attr("id", "provider-select")
                .attr("name", "search_provider")
                .attr("class", "provider-selector");

            for provider in &sage_config.available_search_providers {
                let is_default = provider == &sage_config.default_search_provider;
                let label = match provider.as_str() {
                    "brave" => "Brave Search",
                    "duckduckgo" => "DuckDuckGo",
                    "searxng" => "SearXNG",
                    "serpapi" => "SerpAPI",
                    _ => provider,
                };
                let mut opt = option().attr("value", provider).text(label);
                if is_default {
                    opt = opt.attr("selected", "selected");
                }
                provider_select = provider_select.child(opt);
            }

            div()
                .class("chat-input-extras")
                .child(div().attr("style", "flex: 1;"))
                .child(provider_select)
                .child(model_select.attr("name", "instance_id"))
        });

    // Sidebar
    let mut sidebar_header = div().class("sidebar-header");

    let mut new_chat_url = with_base_path("/ui/home");
    if let Some(ref pid) = project_id {
        new_chat_url = format!("{}?project_id={}", new_chat_url, pid);
    }

    sidebar_header = sidebar_header.child(
        a().class("new-chat-btn")
            .attr("href", new_chat_url)
            .child(i().class("fas fa-plus"))
            .child(span().text("New Chat")),
    );

    let mut history_list = div().class("history-list").attr("id", "history-list");

    // Projects Section
    let has_projects = !projects.is_empty();
    let projects_open_class = if has_projects { "open" } else { "" };

    let projects_header = div()
        .class(format!("history-section-header collapsible {}", projects_open_class))
        .attr("onclick", "this.classList.toggle('open'); const content = this.nextElementSibling; if(content) { content.classList.toggle('hidden'); }")
        .child(
            div()
                .attr("style", "display: flex; align-items: center; gap: 0.5rem;")
                .child(i().class("fas fa-chevron-right chevron"))
                .child(span().text("Projects"))
        )
        .child(
            button()
                .class("branch-btn")
                .attr("onclick", "event.stopPropagation();") // Prevent toggling when clicking New
                .attr("hx-get", with_base_path("/ui/projects/new-modal"))
                .attr("hx-target", "body")
                .attr("hx-swap", "beforeend")
                .child(i().class("fas fa-plus"))
                .child(span().text("New")),
        );

    history_list = history_list.child(projects_header);

    let mut projects_content = div().class("history-section-content");
    if !has_projects {
        projects_content = projects_content.class("hidden");
    }

    for project in &projects {
        let is_active = Some(project.id.clone()) == project_id;
        let item_class = if is_active {
            "history-item active project-item"
        } else {
            "history-item project-item"
        };
        let link_class = if is_active {
            "history-item-link active"
        } else {
            "history-item-link"
        };

        let icon_class = if is_active {
            "fas fa-folder-open"
        } else {
            "fas fa-folder"
        };

        let item = div().class(item_class).child(
            a().class(link_class)
                .attr(
                    "href",
                    with_base_path(&format!("/ui/home?project_id={}", project.id)),
                )
                .child(i().class(icon_class).attr("style", "margin-right: 8px;"))
                .child(span().text(&project.name)),
        );
        projects_content = projects_content.child(item);

        let project_convs: Vec<_> = conversations
            .iter()
            .filter(|c| c.project_id.as_deref() == Some(&project.id))
            .collect();
        for conv in project_convs {
            let is_conv_active = conv.id == active_id;
            let conv_item_class = if is_conv_active {
                "history-item active project-conv-item"
            } else {
                "history-item project-conv-item"
            };
            let conv_link_class = if is_conv_active {
                "history-item-link active"
            } else {
                "history-item-link"
            };
            let item_id = format!("history-item-{}", conv.id);
            let conv_url = format!(
                "/ui/home?conversation_id={}&project_id={}",
                conv.id, project.id
            );

            let conv_item = div()
                .class(conv_item_class)
                .attr("id", &item_id)
                .child(
                    a().class(conv_link_class)
                        .attr("href", with_base_path(&conv_url))
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
            projects_content = projects_content.child(conv_item);
        }
    }

    history_list = history_list.child(projects_content);

    // Conversations Section
    let conv_header_text = "History";

    history_list = history_list.child(
        div()
            .class("history-section-header collapsible open")
            .attr("style", "margin-top: 0.75rem;")
            .attr("onclick", "this.classList.toggle('open'); const content = this.nextElementSibling; if(content) { content.classList.toggle('hidden'); }")
            .child(
                div()
                    .attr("style", "display: flex; align-items: center; gap: 0.5rem;")
                    .child(i().class("fas fa-chevron-right chevron"))
                    .child(span().text(conv_header_text))
            )
    );

    let mut global_content = div().class("history-section-content");

    let global_convs: Vec<_> = conversations
        .iter()
        .filter(|c| c.project_id.is_none())
        .collect();
    for conv in global_convs {
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

        let conv_url = format!("/ui/home?conversation_id={}", conv.id);

        let item = div()
            .class(item_class)
            .attr("id", &item_id)
            .child(
                a().class(link_class)
                    .attr("href", with_base_path(&conv_url))
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
        global_content = global_content.child(item);
    }

    history_list = history_list.child(global_content);

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
                    .attr(
                        "hx-get",
                        with_base_path(&format!("/ui/chat/edit-form/{}", msg.id)),
                    )
                    .attr("hx-target", format!("#user-{}", msg.id))
                    .attr("hx-swap", "innerHTML")
                    .child(i().class("fas fa-edit"))
                    .child(span().text(" Edit"))
            });

            let branch_widget_opt = (total_siblings > 1
                || regenerate_btn_opt.is_some()
                || edit_btn_opt.is_some())
            .then(|| {
                let mut controls = div().class("branch-controls");

                // ONLY SHOW NAVIGATION FOR ASSISTANT MESSAGES
                if total_siblings > 1 && msg.role == "assistant" {
                    let prev_index = if sibling_index == 0 {
                        total_siblings - 1
                    } else {
                        sibling_index - 1
                    };
                    let next_index = if sibling_index == total_siblings - 1 {
                        0
                    } else {
                        sibling_index + 1
                    };
                    let prev_sibling = &siblings[prev_index];
                    let next_sibling = &siblings[next_index];

                    let nav = div()
                        .class("branch-nav")
                        .child(
                            form()
                                .attr("hx-post", with_base_path("/ui/chat/conversations/switch"))
                                .attr("style", "display: inline;")
                                .child(
                                    input()
                                        .attr("type", "hidden")
                                        .attr("name", "conversation_id")
                                        .attr("value", &msg.conversation_id),
                                )
                                .child(
                                    input()
                                        .attr("type", "hidden")
                                        .attr("name", "target_message_id")
                                        .attr("value", &prev_sibling.id),
                                )
                                .child(
                                    button()
                                        .class("branch-btn")
                                        .attr("type", "submit")
                                        .child(i().class("fas fa-chevron-left")),
                                ),
                        )
                        .child(span().class("branch-info").text(format!(
                            "{}/{}",
                            sibling_index + 1,
                            total_siblings
                        )))
                        .child(
                            form()
                                .attr("hx-post", with_base_path("/ui/chat/conversations/switch"))
                                .attr("style", "display: inline;")
                                .child(
                                    input()
                                        .attr("type", "hidden")
                                        .attr("name", "conversation_id")
                                        .attr("value", &msg.conversation_id),
                                )
                                .child(
                                    input()
                                        .attr("type", "hidden")
                                        .attr("name", "target_message_id")
                                        .attr("value", &next_sibling.id),
                                )
                                .child(
                                    button()
                                        .class("branch-btn")
                                        .attr("type", "submit")
                                        .child(i().class("fas fa-chevron-right")),
                                ),
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
                            {
                                let mut f = form()
                                    .attr("hx-post", with_base_path("/ui/chat/send"))
                                    .attr("hx-target", ".chat-history")
                                    .attr("hx-swap", "beforeend")
                                    .attr("hx-on::after-request", "if(event.detail.successful) { document.getElementById('chat-input').value = ''; document.getElementById('chat-input').style.height = 'auto'; const history = document.querySelector('.chat-history'); history.scrollTop = history.scrollHeight; }")
                                    .class("chat-input-wrapper")
                                    .child(input().attr("type", "hidden").attr("name", "conversation_id").attr("value", &active_id));

                                if let Some(ref pid) = project_id {
                                    f = f.child(input().attr("type", "hidden").attr("name", "project_id").attr("value", pid));
                                }

                                f.child(input_area_container)
                            }
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


                    // Handle favorite/archive button clicks - update locally without reload
                    document.addEventListener('htmx:afterSwap', function(event) {
                        // Only refresh for conversation action endpoints (favorite/archive)
                        if (event.detail.xhr && event.detail.xhr.responseURL &&
                            (event.detail.xhr.responseURL.includes('/favorite') ||
                             event.detail.xhr.responseURL.includes('/archive'))) {
                            // Update the icon without full page reload
                            const button = event.detail.target;
                            if (button && button.classList.contains('conv-action-btn')) {
                                const icon = button.querySelector('i');
                                if (icon) {
                                    icon.classList.toggle('fas');
                                    icon.classList.toggle('far');
                                }
                            }
                        }
                    });
                })();
            "#.to_string()).raw()
        ),
        UiPageKind::Home,
    )
}
