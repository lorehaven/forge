use actix_web::web;
use sage_service::clients::switchboard::VllmInstance;
use sage_service::domain::models::{Conversation, Message, Project};
use sage_service::routers::ui::pages::home::{conv_title_link, render_home_page};
use std::collections::HashMap;

fn instance(id: &str, task: Option<&str>) -> VllmInstance {
    VllmInstance {
        id: id.to_string(),
        namespace: "ns".to_string(),
        model: format!("model-{id}"),
        host: "localhost".to_string(),
        port: 8000,
        quantization: None,
        max_model_len: None,
        gpu_memory_utilization: None,
        enable_prefix_caching: false,
        task: task.map(str::to_string),
        started_at: chrono::Utc::now(),
        status: "running".to_string(),
    }
}

fn sage_config() -> web::Data<sage_service::config::SageConfig> {
    web::Data::new(sage_service::config::SageConfig {
        system_prompt: "sys".to_string(),
        default_models: Vec::new(),
        supported_models: Vec::new(),
        default_search_provider: "duckduckgo".to_string(),
        available_search_providers: vec!["duckduckgo".to_string(), "brave".to_string()],
        capability_profile: sage_service::tools::capabilities::get_profile("web_assistant")
            .expect("web_assistant profile exists"),
        stop_models_on_shutdown: false,
    })
}

fn message(id: &str, role: &str, content: &str, conversation_id: &str) -> Message {
    Message {
        id: id.to_string(),
        conversation_id: conversation_id.to_string(),
        parent_id: None,
        role: role.to_string(),
        content: content.to_string(),
        created_at: "2026-01-01T00:00:00Z".to_string(),
    }
}

async fn render(
    instances_res: anyhow::Result<Vec<VllmInstance>>,
    projects: Vec<Project>,
    conversations: Vec<Conversation>,
    active_messages: Vec<(Message, Vec<Message>)>,
    project_id: Option<String>,
) -> String {
    let resp = render_home_page(
        instances_res,
        projects,
        conversations,
        "active-conv".to_string(),
        active_messages,
        HashMap::new(),
        HashMap::new(),
        Vec::new(),
        None,
        project_id,
        sage_config(),
    );
    let body = actix_web::body::to_bytes(resp.into_body())
        .await
        .unwrap_or_default();
    String::from_utf8_lossy(&body).into_owned()
}

#[test]
fn conv_title_link_falls_back_to_new_chat_for_a_blank_title() {
    let link = quench_web::prelude::a();
    let rendered = conv_title_link(link, "   ").render();
    assert!(rendered.contains("New chat"));
}

#[test]
fn conv_title_link_uses_the_title_when_present() {
    let link = quench_web::prelude::a();
    let rendered = conv_title_link(link, "My conversation").render();
    assert!(rendered.contains("My conversation"));
}

#[actix_web::test]
async fn render_home_page_shows_the_welcome_message_with_no_active_conversation() {
    let html = render(
        Ok(vec![instance("i1", None)]),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        None,
    )
    .await;
    assert!(html.contains("ui_chat_welcome_message"));
    assert!(!html.contains("disabled=\"disabled\""));
}

#[actix_web::test]
async fn render_home_page_disables_the_composer_without_a_chat_capable_model() {
    // Only an embedding-task instance is available, which the chat selector must exclude.
    let html = render(
        Ok(vec![instance("i1", Some("embed"))]),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        None,
    )
    .await;
    assert!(html.contains("ui_chat_no_models"));
    assert!(html.contains("no-model-warning"));
}

#[actix_web::test]
async fn render_home_page_shows_a_switchboard_unavailable_message_on_error() {
    let html = render(
        Err(anyhow::anyhow!("connection refused")),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        None,
    )
    .await;
    assert!(html.contains("ui_chat_switchboard_unavailable"));
}

#[actix_web::test]
async fn render_home_page_lists_conversations_and_marks_the_active_one() {
    let conversations = vec![
        Conversation {
            id: "active-conv".to_string(),
            title: "Active chat".to_string(),
            active_message_id: None,
            owner: "admin".to_string(),
            project_id: None,
            updated_at: "2026-01-02T00:00:00Z".to_string(),
        },
        Conversation {
            id: "other-conv".to_string(),
            title: "Other chat".to_string(),
            active_message_id: None,
            owner: "admin".to_string(),
            project_id: None,
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        },
    ];
    let html = render(
        Ok(vec![instance("i1", None)]),
        Vec::new(),
        conversations,
        Vec::new(),
        None,
    )
    .await;
    assert!(html.contains("Active chat"));
    assert!(html.contains("Other chat"));
    assert!(html.contains("history-item-active-conv"));
}

#[actix_web::test]
async fn render_home_page_renders_message_history_with_branch_controls_for_siblings() {
    let root = message("root", "user", "hello", "active-conv");
    let sib_a = message("a", "assistant", "answer a", "active-conv");
    let sib_b = message("b", "assistant", "answer b", "active-conv");
    let active_messages = vec![(root, vec![]), (sib_b.clone(), vec![sib_a, sib_b])];
    let html = render(
        Ok(vec![instance("i1", None)]),
        Vec::new(),
        Vec::new(),
        active_messages,
        None,
    )
    .await;
    assert!(html.contains("hello"));
    assert!(html.contains("answer b"));
    assert!(html.contains("branch-nav"));
    assert!(html.contains("1/2") || html.contains("2/2"));
}

#[actix_web::test]
async fn render_home_page_shows_a_project_and_its_files_when_active() {
    let project = Project {
        id: "p1".to_string(),
        name: "My Project".to_string(),
        owner: "admin".to_string(),
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
    };
    let html = render(
        Ok(vec![instance("i1", None)]),
        vec![project],
        Vec::new(),
        Vec::new(),
        Some("p1".to_string()),
    )
    .await;
    assert!(html.contains("My Project"));
    assert!(html.contains("project_id=p1"));
}

#[actix_web::test]
async fn render_home_page_includes_every_configured_search_provider() {
    let html = render(
        Ok(vec![instance("i1", None)]),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        None,
    )
    .await;
    assert!(html.contains("DuckDuckGo"));
    assert!(html.contains("Brave Search"));
}
