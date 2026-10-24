use crate::clients::switchboard::{SwitchboardClient, VllmInstance};
use crate::domain::models::Conversation;
use crate::routers::ui::chat::{
    ChatRequest, ChatState, get_conversation_message_nodes, get_siblings,
};
use crate::routers::ui::common::{ActorOrRedirect, render_page_fill};
use quench_db::prelude::{Crud, Db};
use quench_http::prelude::{Inject, Query, Response, get, http::StatusCode};
use quench_starter::common::routes::with_base_path;
use quench_web::prelude::*;
use std::sync::Arc;

#[derive(Debug, quench_serde::Deserialize)]
pub struct HomeQuery {
    pub conversation_id: Option<String>,
    pub project_id: Option<String>,
}

#[allow(clippy::too_many_arguments)]
async fn handle_home_page(
    username: String,
    switchboard: Arc<SwitchboardClient>,
    db: Arc<Db>,
    chat_state: Arc<ChatState>,
    sage_config: Arc<crate::config::SageConfig>,
    query: HomeQuery,
) -> Response {
    let instances = switchboard.get_vllm_instances().await;

    // Every configured default model must be running before the chat UI is usable; until then,
    // redirect to the initializing screen, which shows launch progress and sends the user back here.
    let models_ready = matches!(&instances, Ok(insts) if crate::routers::ui::pages::initializing::all_models_running(&sage_config.default_models, insts));
    if !models_ready {
        return Response::new(StatusCode::FOUND)
            .header("Location", with_base_path("/ui/initializing"));
    }

    // Fetch user's projects
    let project_repo = db.repository::<crate::domain::models::Project>();
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
        .unwrap_or_else(|| quench_core::uuid::Uuid::new_v4().to_string());

    let mut active_messages = Vec::new();
    let active_message_id: Option<String> = if let Some(ref cid) = query.conversation_id
        && let Ok(Some(conv)) = conv_repo.read(cid).await
    {
        conv.active_message_id
    } else {
        None
    };

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

    // Auto-trigger an AI response if the last message is from the user.
    let auto_trigger_ai = if let Some((last_msg, _)) = active_messages.last()
        && last_msg.role == "user"
        // Find an available model instance
        && let Ok(ref insts) = instances
        && let Some(instance) = insts.first()
    {
        let pending_id = quench_core::uuid::Uuid::new_v4().to_string();
        let chat_req = ChatRequest {
            instance_id: instance.id.clone(),
            message: last_msg.content.clone(),
            conversation_id: last_msg.conversation_id.clone(),
            project_id: query.project_id.clone(),
            parent_id: Some(last_msg.id.clone()),
            skip_user_message: true, // DB message already exists
            search_provider: None,
            capability_profile: None,
            tool_confirmations: Vec::new(),
            file_ids: String::new(),
        };
        chat_state
            .pending_messages
            .insert(pending_id.clone(), chat_req);
        Some(pending_id)
    } else {
        None
    };

    // Load RAG source attribution for the assistant messages on screen.
    let assistant_ids: Vec<String> = active_messages
        .iter()
        .filter(|(m, _)| m.role == "assistant")
        .map(|(m, _)| m.id.clone())
        .collect();
    let sources_by_message =
        crate::files::rag::load_sources_for_messages(&db, &assistant_ids).await;

    // Load file attachments for the user messages on screen.
    let user_ids: Vec<String> = active_messages
        .iter()
        .filter(|(m, _)| m.role == "user")
        .map(|(m, _)| m.id.clone())
        .collect();
    let attachments_by_message = crate::routers::files::files_by_message(&db, &user_ids).await;

    // Files across the active project power the sidebar "Files" section; only load when an owned project is open.
    let project_files = match &query.project_id {
        Some(pid) if projects.iter().any(|p| &p.id == pid) => {
            crate::routers::files::visible_files_for_project(&db, pid)
                .await
                .unwrap_or_default()
        }
        _ => Vec::new(),
    };

    render_home_page(&HomePage {
        instances,
        projects,
        conversations,
        active_id,
        active_messages,
        sources_by_message,
        attachments_by_message,
        project_files,
        auto_trigger_ai,
        project_id: query.project_id.clone(),
        sage_config,
    })
}

#[get("/ui/home")]
pub async fn home(
    actor: ActorOrRedirect,
    Inject(switchboard): Inject<SwitchboardClient>,
    Inject(db): Inject<Db>,
    Inject(chat_state): Inject<ChatState>,
    Inject(sage_config): Inject<crate::config::SageConfig>,
    Query(query): Query<HomeQuery>,
) -> Response {
    let claims = match actor.or_redirect() {
        Ok(claims) => claims,
        Err(response) => return response,
    };
    handle_home_page(claims.sub, switchboard, db, chat_state, sage_config, query).await
}

#[get("/ui/home/")]
pub async fn home_slash(
    actor: ActorOrRedirect,
    Inject(switchboard): Inject<SwitchboardClient>,
    Inject(db): Inject<Db>,
    Inject(chat_state): Inject<ChatState>,
    Inject(sage_config): Inject<crate::config::SageConfig>,
    Query(query): Query<HomeQuery>,
) -> Response {
    let claims = match actor.or_redirect() {
        Ok(claims) => claims,
        Err(response) => return response,
    };
    handle_home_page(claims.sub, switchboard, db, chat_state, sage_config, query).await
}

/// Sidebar conversation link, falling back to a localized "New chat" label for conversations
/// created lazily by attaching a file before the first message is sent (blank stored title).
#[must_use]
pub fn conv_title_link(link: Element, title: &str) -> Element {
    if title.trim().is_empty() {
        link.attr("data-i18n", "ui_chat_untitled").text("New chat")
    } else {
        link.text(title)
    }
}

/// Everything the home page shows.
#[derive(Debug)]
pub struct HomePage {
    pub instances: quench_core::Result<Vec<VllmInstance>>,
    pub projects: Vec<crate::domain::models::Project>,
    pub conversations: Vec<Conversation>,
    pub active_id: String,
    pub active_messages: Vec<(
        crate::domain::models::Message,
        Vec<crate::domain::models::Message>,
    )>,
    pub sources_by_message: std::collections::HashMap<String, Vec<crate::files::rag::RagSource>>,
    pub attachments_by_message: std::collections::HashMap<String, Vec<crate::domain::models::File>>,
    pub project_files: Vec<crate::domain::models::File>,
    pub auto_trigger_ai: Option<String>,
    pub project_id: Option<String>,
    pub sage_config: Arc<crate::config::SageConfig>,
}

/// The model picker: chat-capable instances, or why there are none.
fn model_selector(
    instances: &quench_core::Result<Vec<VllmInstance>>,
    chat_instances: &[&VllmInstance],
) -> Element {
    let mut model_select = select().class("model-selector").attr("id", "model-select");

    match instances {
        Ok(_) => {
            if chat_instances.is_empty() {
                model_select = model_select.child(
                    option()
                        .attr("value", "")
                        .attr("disabled", "disabled")
                        .attr("selected", "selected")
                        .attr("data-i18n", "ui_chat_no_models")
                        .text("No models available"),
                );
            } else {
                for instance in chat_instances {
                    model_select = model_select
                        .child(option().attr("value", &instance.id).text(&instance.model));
                }
            }
        }
        Err(err) => {
            quench_log::error!("Failed to fetch models from switchboard: {}", err);
            model_select = model_select.child(
                option()
                    .attr("value", "")
                    .attr("disabled", "disabled")
                    .attr("selected", "selected")
                    .attr("data-i18n", "ui_chat_switchboard_unavailable")
                    .text("Switchboard unavailable"),
            );
        }
    }
    model_select
}

/// The search provider picker, with the configured default selected.
fn provider_selector(sage_config: &crate::config::SageConfig) -> Element {
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
    provider_select
}

/// Paperclip upload control: the hidden file input POSTs to /ui/files/attach and appends a chip to
/// #pending-attachments; hx-include pulls scope from the composer's hidden inputs.
fn attach_area(has_model: bool) -> Element {
    let attach_input = input()
        .attr("type", "file")
        .attr("id", "composer-file-input")
        .attr("name", "file")
        .attr("accept", crate::routers::files::upload_accept_attribute())
        .attr("style", "display: none;")
        .attr("hx-post", with_base_path("/ui/files/attach"))
        .attr("hx-encoding", "multipart/form-data")
        .attr(
            "hx-include",
            "#composer-conversation-id, #composer-project-id",
        )
        .attr("hx-target", "#pending-attachments")
        .attr("hx-swap", "beforeend")
        .attr("hx-trigger", "change")
        .attr("hx-on::after-request", "this.value = '';");

    let attach_btn = label()
        .class("chat-attach-btn")
        .attr("for", "composer-file-input")
        .attr(
            "title",
            "Attach a file (images, pdf, txt, csv, md, html, json, yaml, source code…)",
        )
        .attr("data-i18n-title", "ui_chat_attach_tooltip")
        .child(i().class("fas fa-paperclip"))
        .child(attach_input);

    let mut attach_area = div().class("chat-attach-area").child(attach_btn);
    if !has_model {
        attach_area = attach_area.class("chat-attach-area disabled");
    }
    attach_area
}

/// The message box and its send button, plus the attach/provider/model row beneath.
fn composer(page: &HomePage, model_select: Element, has_model: bool) -> Element {
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

    input_area_container
        // Staging area for files attached to the next message.
        .child(
            div()
                .attr("id", "pending-attachments")
                .class("pending-attachments"),
        )
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
                .child(attach_area(has_model))
                .child(div().attr("style", "flex: 1;"))
                .child(provider_selector(&page.sage_config))
                .child(model_select.attr("name", "instance_id")),
        )
}

/// A conversation's `⋮` menu, with its delete action.
pub(crate) fn conversation_menu(conv_id: &str, active_id: &str) -> Element {
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
                            "/ui/chat/conversations/delete-modal/{conv_id}?active_id={active_id}"
                        )),
                    )
                    .attr("hx-target", "#confirm-delete-modal")
                    .attr("hx-swap", "outerHTML")
                    .child(i().class("fas fa-trash"))
                    .child(span().attr("data-i18n", "ui_common_delete").text("Delete")),
            ),
        )
}

/// One conversation in the sidebar, linking to `conv_url`; `suffix` adds to the item's classes.
fn conversation_item(
    conv: &Conversation,
    conv_url: &str,
    active_id: &str,
    suffix: &str,
) -> Element {
    let is_active = conv.id == active_id;
    let item_class = format!(
        "history-item{}{suffix}",
        if is_active { " active" } else { "" }
    );
    let link_class = if is_active {
        "history-item-link active"
    } else {
        "history-item-link"
    };

    div()
        .class(item_class)
        .attr("id", format!("history-item-{}", conv.id))
        .child(conv_title_link(
            a().class(link_class).attr("href", with_base_path(conv_url)),
            &conv.title,
        ))
        .child(conversation_menu(&conv.id, active_id))
}

const TOGGLE_SECTION: &str = "this.classList.toggle('open'); const content = this.nextElementSibling; if(content) { content.classList.toggle('hidden'); }";

/// The "Projects" header with its New button.
pub(crate) fn projects_header(has_projects: bool) -> Element {
    let projects_open_class = if has_projects { "open" } else { "" };
    div()
        .class(format!(
            "history-section-header collapsible {projects_open_class}"
        ))
        .attr("onclick", TOGGLE_SECTION)
        .child(
            div()
                .attr("style", "display: flex; align-items: center; gap: 0.5rem;")
                .child(i().class("fas fa-chevron-right chevron"))
                .child(
                    span()
                        .attr("data-i18n", "ui_sidebar_projects")
                        .text("Projects"),
                ),
        )
        .child(
            button()
                .class("branch-btn")
                .attr("onclick", "event.stopPropagation();") // Prevent toggling when clicking New
                .attr("hx-get", with_base_path("/ui/projects/new-modal"))
                .attr("hx-target", "body")
                .attr("hx-swap", "beforeend")
                .child(i().class("fas fa-plus"))
                .child(span().attr("data-i18n", "ui_sidebar_new").text("New")),
        )
}

/// Each project, with (for the open one) its files, and the project's conversations beneath.
fn projects_content(page: &HomePage) -> Element {
    let mut projects_content = div().class("history-section-content");
    if page.projects.is_empty() {
        projects_content = projects_content.class("hidden");
    }

    for project in &page.projects {
        let is_active = Some(project.id.clone()) == page.project_id;
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

        // For the open project, list its uploaded files above its conversations.
        if is_active {
            projects_content = projects_content.child(
                crate::routers::ui::pages::files::render_project_files_section(&page.project_files),
            );
        }

        for conv in page
            .conversations
            .iter()
            .filter(|c| c.project_id.as_deref() == Some(&project.id))
        {
            let conv_url = format!(
                "/ui/home?conversation_id={}&project_id={}",
                conv.id, project.id
            );
            projects_content = projects_content.child(conversation_item(
                conv,
                &conv_url,
                &page.active_id,
                " project-conv-item",
            ));
        }
    }
    projects_content
}

/// The "History" header.
pub(crate) fn history_header() -> Element {
    div()
        .class("history-section-header collapsible open")
        .attr("style", "margin-top: 0.75rem;")
        .attr("onclick", TOGGLE_SECTION)
        .child(
            div()
                .attr("style", "display: flex; align-items: center; gap: 0.5rem;")
                .child(i().class("fas fa-chevron-right chevron"))
                .child(
                    span()
                        .attr("data-i18n", "ui_sidebar_history")
                        .text("History"),
                ),
        )
}

/// The "History" header and the conversations that belong to no project.
fn history_section(page: &HomePage) -> [Element; 2] {
    let header = history_header();

    let mut global_content = div().class("history-section-content");
    for conv in page.conversations.iter().filter(|c| c.project_id.is_none()) {
        let conv_url = format!("/ui/home?conversation_id={}", conv.id);
        global_content =
            global_content.child(conversation_item(conv, &conv_url, &page.active_id, ""));
    }
    [header, global_content]
}

/// The new-chat button, the projects and the conversation history.
fn sidebar(page: &HomePage) -> Element {
    let mut new_chat_url = with_base_path("/ui/home");
    if let Some(ref pid) = page.project_id {
        new_chat_url = format!("{new_chat_url}?project_id={pid}");
    }

    let sidebar_header = div().class("sidebar-header").child(
        a().class("new-chat-btn")
            .attr("href", new_chat_url)
            .child(i().class("fas fa-plus"))
            .child(
                span()
                    .attr("data-i18n", "ui_sidebar_new_chat")
                    .text("New Chat"),
            ),
    );

    let [history_header, global_content] = history_section(page);
    let history_list = div()
        .class("history-list")
        .attr("id", "history-list")
        .child(projects_header(!page.projects.is_empty()))
        .child(projects_content(page))
        .child(history_header)
        .child(global_content);

    div()
        .class("history-sidebar")
        .child(sidebar_header)
        .child(history_list)
}

const SCROLL_TO_TARGET: &str = "const target = document.getElementById(this.dataset.msgId); if (target) { target.scrollIntoView({behavior: 'smooth', block: 'start'}); }";

/// The greeting shown when there is no conversation yet, and its dot in the navigator.
fn welcome_message() -> (Element, Element) {
    let message = div()
        .class("chat-message message-ai")
        .attr("id", "msg-0")
        .child(
            div().class("message-inner").child(
                div()
                    .class("message-content")
                    .attr("data-i18n", "ui_chat_welcome_message"),
            ),
        );

    let dot = div()
        .class("nav-dot active")
        .attr("data-msg-id", "msg-0")
        .attr("onclick", SCROLL_TO_TARGET)
        .child(
            div()
                .class("nav-tooltip")
                .attr("data-i18n", "ui_chat_welcome_tooltip")
                .text("Hello! I am Sage..."),
        );
    (message, dot)
}

/// A sibling-switch form: posts `conversation_id` and the `target_message_id` to move to.
pub(crate) fn switch_form(conversation_id: &str, target_message_id: &str, icon: &str) -> Element {
    form()
        .attr("hx-post", with_base_path("/ui/chat/conversations/switch"))
        .attr("style", "display: inline;")
        .child(
            input()
                .attr("type", "hidden")
                .attr("name", "conversation_id")
                .attr("value", conversation_id),
        )
        .child(
            input()
                .attr("type", "hidden")
                .attr("name", "target_message_id")
                .attr("value", target_message_id),
        )
        .child(
            button()
                .class("branch-btn")
                .attr("type", "submit")
                .child(i().class(icon)),
        )
}

/// Previous/next between the alternatives (siblings) of an assistant message, plus regenerate / edit.
fn branch_controls(
    msg: &crate::domain::models::Message,
    siblings: &[crate::domain::models::Message],
    is_last: bool,
    is_last_user: bool,
) -> Option<Element> {
    let total_siblings = siblings.len();
    let sibling_index = siblings.iter().position(|s| s.id == msg.id).unwrap_or(0);

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

    if total_siblings <= 1 && regenerate_btn_opt.is_none() && edit_btn_opt.is_none() {
        return None;
    }

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

        controls = controls.child(
            div()
                .class("branch-nav")
                .child(switch_form(
                    &msg.conversation_id,
                    &siblings[prev_index].id,
                    "fas fa-chevron-left",
                ))
                .child(span().class("branch-info").text(format!(
                    "{}/{}",
                    sibling_index + 1,
                    total_siblings
                )))
                .child(switch_form(
                    &msg.conversation_id,
                    &siblings[next_index].id,
                    "fas fa-chevron-right",
                )),
        );
    }

    if let Some(btn) = regenerate_btn_opt {
        controls = controls.child(btn);
    }

    if let Some(btn) = edit_btn_opt {
        controls = controls.child(btn);
    }

    Some(controls)
}

/// The "Sage is thinking" placeholder that streams the answer in, and its dot in the navigator.
fn thinking_message(pending_id: &str) -> (Element, Element) {
    let stream_url = with_base_path(&format!("/ui/chat/stream/{pending_id}"));
    let message = div()
        .class("chat-message message-ai")
        .attr("id", format!("ai-{pending_id}"))
        .attr("hx-ext", "sse")
        .attr("sse-connect", stream_url)
        .attr("sse-swap", "message")
        .child(
            div().class("message-inner").child(
                div()
                    .class("message-content")
                    .attr("data-i18n", "ui_chat_thinking")
                    .text("Sage is thinking..."),
            ),
        );

    let dot = div()
        .class("nav-dot active")
        .attr("id", format!("dot-ai-{pending_id}"))
        .attr("data-msg-id", format!("ai-{pending_id}"))
        .attr("onclick", SCROLL_TO_TARGET)
        .child(
            div()
                .class("nav-tooltip")
                .attr("id", format!("tooltip-ai-{pending_id}"))
                .attr("data-i18n", "ui_chat_thinking")
                .text("Sage is thinking..."),
        );
    (message, dot)
}

/// The conversation's messages, and the dots that navigate between them.
fn chat_history(page: &HomePage) -> (Element, Element) {
    use crate::routers::ui::common::format::format_message;

    let mut history_div = div()
        .class("chat-history")
        .attr(
            "hx-on:htmx:sse-message",
            "this.scrollTop = this.scrollHeight;",
        )
        .child(div().class("chat-history-spacer"));

    let mut nav_div = div().class("chat-navigation");

    if page.active_messages.is_empty() {
        let (message, dot) = welcome_message();
        return (history_div.child(message), nav_div.child(dot));
    }

    let last_user_idx = page
        .active_messages
        .iter()
        .rposition(|(m, _)| m.role == "user");

    for (idx, (msg, siblings)) in page.active_messages.iter().enumerate() {
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

        let is_last = idx == page.active_messages.len() - 1;
        let is_last_user = Some(idx) == last_user_idx;
        let branch_widget_opt = branch_controls(msg, siblings, is_last, is_last_user);

        let sources_opt = page
            .sources_by_message
            .get(&msg.id)
            .and_then(|s| crate::routers::ui::common::format::render_sources(s));

        let attachments_opt = page
            .attachments_by_message
            .get(&msg.id)
            .and_then(|files| crate::routers::ui::pages::files::render_attachments_row(files));

        let chat_msg = div()
            .class(format!("chat-message {role_class}"))
            .attr("id", &element_id)
            .child(
                div()
                    .class("message-inner")
                    .raw()
                    .text(format_message(trimmed_content))
                    .child_opt(attachments_opt)
                    .child_opt(sources_opt)
                    .child_opt(branch_widget_opt),
            );
        history_div = history_div.child(chat_msg);

        let preview_raw: String = trimmed_content.chars().take(30).collect();
        let preview = if trimmed_content.chars().count() > 30 {
            format!("{preview_raw}...")
        } else {
            preview_raw
        };

        let dot = div()
            .class("nav-dot")
            .attr("data-msg-id", &element_id)
            .attr("onclick", SCROLL_TO_TARGET)
            .child(div().class("nav-tooltip").text(preview));
        nav_div = nav_div.child(dot);
    }

    // If we are auto-triggering AI (e.g. after an edit or branching)
    if let Some(pending_id) = &page.auto_trigger_ai {
        let (message, dot) = thinking_message(pending_id);
        history_div = history_div.child(message);
        nav_div = nav_div.child(dot);
    }
    (history_div, nav_div)
}

/// The message form: its hidden scope inputs and the composer.
fn chat_form(page: &HomePage, composer: Element) -> Element {
    let mut f = form()
        .attr("hx-post", with_base_path("/ui/chat/send"))
        .attr("hx-target", ".chat-history")
        .attr("hx-swap", "beforeend")
        // Staged ids join into one comma-separated `file_ids` param;
        // serde_urlencoded can't parse repeated keys into a Vec.
        .attr("hx-on::config-request", "if (event.detail.path && event.detail.path.indexOf('/ui/chat/send') !== -1) { event.detail.parameters['file_ids'] = Array.from(document.querySelectorAll('#pending-attachments .attachment-chip')).map(function(c){ return c.getAttribute('data-file-id'); }).filter(Boolean).join(','); }")
        // Guarded to the send path: htmx:afterRequest bubbles from /ui/files/attach too.
        .attr("hx-on::after-request", "if(event.detail.successful && event.detail.xhr && event.detail.xhr.responseURL.indexOf('/ui/chat/send') !== -1) { document.getElementById('chat-input').value = ''; document.getElementById('chat-input').style.height = 'auto'; const pending = document.getElementById('pending-attachments'); if (pending) pending.innerHTML = ''; const history = document.querySelector('.chat-history'); history.scrollTop = history.scrollHeight; }")
        .class("chat-input-wrapper")
        .child(input().attr("type", "hidden").attr("id", "composer-conversation-id").attr("name", "conversation_id").attr("value", &page.active_id));

    if let Some(ref pid) = page.project_id {
        f = f.child(
            input()
                .attr("type", "hidden")
                .attr("id", "composer-project-id")
                .attr("name", "project_id")
                .attr("value", pid),
        );
    }

    f.child(composer)
}

/// Keeps the history scrolled to the newest message and the navigator dots in step with it.
const HOME_SCRIPT: &str = r"
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
            ";

#[must_use]
pub fn render_home_page(page: &HomePage) -> Response {
    // Only chat-capable instances belong in the selector; embedding instances would 404 on chat completions.
    let chat_instances: Vec<&VllmInstance> = page.instances.as_ref().map_or_else(
        |_| Vec::new(),
        |instances| instances.iter().filter(|i| i.is_chat_capable()).collect(),
    );
    let has_model = !chat_instances.is_empty();

    let model_select = model_selector(&page.instances, &chat_instances);
    let composer = composer(page, model_select, has_model);
    let (history_div, nav_div) = chat_history(page);

    render_page_fill(
        StatusCode::OK,
        content()
            .class("home-content")
            .child(
                div().class("sage-layout").child(
                    div()
                        .class("chat-container")
                        .child(history_div)
                        .child(nav_div)
                        .child(chat_form(page, composer)),
                ),
            )
            .child(
                div()
                    .attr("id", "confirm-delete-modal")
                    .class("estimates-modal"),
            )
            .child(script(HOME_SCRIPT.to_string()).raw()),
        sidebar(page),
    )
}

pub fn register_routes() {
    let _ = home as fn(_, _, _, _, _, _) -> _;
    let _ = home_slash as fn(_, _, _, _, _, _) -> _;
}
