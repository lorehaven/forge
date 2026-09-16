//! HTTP-level coverage for `home`/`home_slash`, which `render_home_page`'s
//! own direct-call tests (`routers_ui_pages_home_render_tests.rs`) bypass
//! entirely. Uses the `wiremock`-backed `SwitchboardClient` harness from
//! `clients_switchboard_tests.rs` so `get_vllm_instances()` succeeds without
//! a real switchboard - with `default_models: Vec::new()` (nothing
//! required), `all_models_running` is vacuously true, so the handler
//! proceeds past the "redirect to /ui/initializing" gate into the rest of
//! its body (loading projects/conversations/messages and rendering).

use crate::env_support::env_lock;
use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode, Uri};
use http_body_util::BodyExt;
use quench_auth::domain::jwt::JwtConfig;
use quench_db::InMemoryDb;
use quench_db::prelude::{Crud, Db};
use quench_http::di::ContainerBuilder;
use quench_http::endpoint::Endpoint;
use quench_http::request::Request;
use sage_service::clients::switchboard::SwitchboardClient;
use sage_service::domain::models::{Conversation, Message, Project};
use sage_service::routers::ui::chat::ChatState;
use std::sync::Arc;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn db() -> Db {
    Db::InMemory(InMemoryDb::new())
}

fn sage_config() -> sage_service::config::SageConfig {
    sage_service::config::SageConfig {
        system_prompt: "sys".to_string(),
        default_models: Vec::new(),
        supported_models: Vec::new(),
        default_search_provider: "duckduckgo".to_string(),
        available_search_providers: vec!["duckduckgo".to_string()],
        capability_profile: sage_service::tools::capabilities::get_profile("web_assistant")
            .expect("web_assistant profile exists"),
        stop_models_on_shutdown: false,
    }
}

fn chat_state() -> ChatState {
    ChatState {
        pending_messages: dashmap::DashMap::new(),
    }
}

/// A `SwitchboardClient` whose `get_vllm_instances()` succeeds against a
/// mocked token + API endpoint, and nothing else registered - only
/// `/api/v1/vllm/instances` is called by `home`'s happy path.
async fn switchboard_returning(instances: serde_json::Value) -> SwitchboardClient {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/token"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(
                serde_json::json!({"access_token": "test-token", "expires_in": 3600}),
            ),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/vllm/instances"))
        .respond_with(ResponseTemplate::new(200).set_body_json(instances))
        .mount(&server)
        .await;

    unsafe {
        std::env::set_var("SWITCHBOARD_URL", server.uri());
        std::env::set_var("GATEHOUSE_URL", server.uri());
        std::env::set_var("CLIENT_SECRET_SAGE_SWITCHBOARD", "test-secret");
    }
    // Leaked deliberately: the client only holds the URL it read at
    // construction time, and the mock server must outlive every request
    // the handler makes against it during the test.
    Box::leak(Box::new(server));
    SwitchboardClient::new()
}

async fn app(
    jwt_config: JwtConfig,
    db: Db,
    switchboard: SwitchboardClient,
) -> (Arc<dyn Endpoint>, Arc<quench_http::di::Container>) {
    sage_service::routers::ui::pages::home::register_routes();
    let container = ContainerBuilder::new()
        .provide(jwt_config)
        .provide(switchboard)
        .provide(db)
        .provide_arc(Arc::new(chat_state()))
        .provide(sage_config())
        .build()
        .await
        .unwrap();
    (
        quench_starter::http::discover_and_mount("/"),
        Arc::new(container),
    )
}

fn req(path: &str, container: &Arc<quench_http::di::Container>) -> Request {
    Request::new(
        Method::GET,
        path.parse::<Uri>().unwrap(),
        HeaderMap::new(),
        quench_http::body::InboundBody::from_bytes(Bytes::new()),
        container.clone(),
    )
}

async fn body_text(resp: quench_http::response::Response) -> String {
    let collected = resp.into_hyper().into_body().collect().await.expect("body");
    String::from_utf8_lossy(&collected.to_bytes()).into_owned()
}

#[allow(clippy::await_holding_lock)] // single-threaded test, held deliberately for env-var safety across the whole call
#[tokio::test]
async fn home_redirects_to_login_when_unauthenticated() {
    let _guard = env_lock().lock().unwrap_or_else(|p| p.into_inner());
    let switchboard = switchboard_returning(serde_json::json!([])).await;
    let mut config = JwtConfig::for_tests();
    config.auth_enabled = true;
    let (app, container) = app(config, db(), switchboard).await;

    let resp = app.call(req("/ui/home", &container)).await;
    assert!(resp.status().is_redirection());
}

#[allow(clippy::await_holding_lock)] // single-threaded test, held deliberately for env-var safety across the whole call
#[tokio::test]
async fn home_renders_the_page_with_no_conversations() {
    let _guard = env_lock().lock().unwrap_or_else(|p| p.into_inner());
    let switchboard = switchboard_returning(serde_json::json!([])).await;
    let (app, container) = app(JwtConfig::for_tests(), db(), switchboard).await;

    let resp = app.call(req("/ui/home", &container)).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[allow(clippy::await_holding_lock)] // single-threaded test, held deliberately for env-var safety across the whole call
#[tokio::test]
async fn home_slash_renders_the_page_too() {
    let _guard = env_lock().lock().unwrap_or_else(|p| p.into_inner());
    let switchboard = switchboard_returning(serde_json::json!([])).await;
    let (app, container) = app(JwtConfig::for_tests(), db(), switchboard).await;

    let resp = app.call(req("/ui/home/", &container)).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[allow(clippy::await_holding_lock)] // single-threaded test, held deliberately for env-var safety across the whole call
#[tokio::test]
async fn home_lists_only_the_callers_own_projects_and_conversations() {
    let _guard = env_lock().lock().unwrap_or_else(|p| p.into_inner());
    let switchboard = switchboard_returning(serde_json::json!([])).await;
    let db = db();
    db.repository::<Project>()
        .create(&Project {
            id: "p1".to_string(),
            name: "Mine".to_string(),
            owner: "admin".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        })
        .await
        .unwrap();
    db.repository::<Project>()
        .create(&Project {
            id: "p2".to_string(),
            name: "Someone else's".to_string(),
            owner: "someone-else".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        })
        .await
        .unwrap();
    db.repository::<Conversation>()
        .create(&Conversation {
            id: "c1".to_string(),
            title: "Chat".to_string(),
            active_message_id: None,
            owner: "admin".to_string(),
            project_id: None,
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        })
        .await
        .unwrap();

    let (app, container) = app(JwtConfig::for_tests(), db, switchboard).await;
    // `render_home_page` only names the *active* project (see
    // `routers_ui_pages_home_render_tests`'s own
    // `render_home_page_shows_a_project_and_its_files_when_active`), so
    // request "Mine" as the active one via `project_id` to see it named.
    let resp = app.call(req("/ui/home?project_id=p1", &container)).await;
    let html = body_text(resp).await;
    assert!(html.contains("Mine"));
    assert!(!html.contains("Someone else's"));
    assert!(html.contains("Chat"));
}

#[allow(clippy::await_holding_lock)] // single-threaded test, held deliberately for env-var safety across the whole call
#[tokio::test]
async fn home_loads_the_active_conversations_message_history() {
    let _guard = env_lock().lock().unwrap_or_else(|p| p.into_inner());
    let switchboard = switchboard_returning(serde_json::json!([])).await;
    let db = db();
    db.repository::<Conversation>()
        .create(&Conversation {
            id: "c1".to_string(),
            title: "Chat".to_string(),
            active_message_id: Some("m1".to_string()),
            owner: "admin".to_string(),
            project_id: None,
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        })
        .await
        .unwrap();
    db.repository::<Message>()
        .create(&Message {
            id: "m1".to_string(),
            conversation_id: "c1".to_string(),
            parent_id: None,
            role: "user".to_string(),
            content: "hello there".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
        })
        .await
        .unwrap();

    let (app, container) = app(JwtConfig::for_tests(), db, switchboard).await;
    let resp = app
        .call(req("/ui/home?conversation_id=c1", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let html = body_text(resp).await;
    assert!(html.contains("hello there"));
}
