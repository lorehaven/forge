//! HTTP-handler and pure-function coverage for `routers/ui/chat.rs`, beyond
//! the branch-selection test already in `routers_ui_chat_tests.rs`.
//!
//! `stream_message` itself is deliberately not covered here: it needs a real
//! `SwitchboardClient`/`VllmClient` pair (instance discovery, then a live
//! chat-completion stream) with no injectable seam short of a production
//! refactor, and is by far the riskiest handler in this file to exercise
//! with fakes given how much phase-to-phase state (tool execution, RAG
//! injection, DB writes, SSE framing) it threads through in one function.
//! Everything else in this file that doesn't need those two clients is
//! covered.

use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode, Uri};
use http_body_util::BodyExt;
use quench_auth::domain::jwt::JwtConfig;
use quench_db::prelude::{Crud, Db};
use quench_http::di::ContainerBuilder;
use quench_http::endpoint::Endpoint;
use quench_http::request::Request;
use sage_service::domain::models::{Conversation, Message};
use sage_service::routers::ui::chat::*;
use std::sync::Arc;

fn db() -> Db {
    Db::InMemory(quench_db::InMemoryDb::new())
}

fn state() -> ChatState {
    ChatState {
        pending_messages: dashmap::DashMap::new(),
    }
}

fn auth_disabled() -> JwtConfig {
    JwtConfig::for_tests()
}

fn auth_enabled() -> JwtConfig {
    let mut config = JwtConfig::for_tests();
    config.auth_enabled = true;
    config
}

fn sage_config() -> sage_service::config::SageConfig {
    sage_service::config::SageConfig {
        system_prompt: "you are sage".to_string(),
        default_models: Vec::new(),
        supported_models: Vec::new(),
        default_search_provider: "duckduckgo".to_string(),
        available_search_providers: vec!["duckduckgo".to_string()],
        capability_profile: sage_service::tools::capabilities::get_profile("web_assistant")
            .expect("web_assistant profile exists"),
        stop_models_on_shutdown: false,
    }
}

async fn app(builder: ContainerBuilder) -> (Arc<dyn Endpoint>, Arc<quench_http::di::Container>) {
    register_routes();
    let container = builder.build().await.unwrap();
    (
        quench_starter::http::discover_and_mount("/"),
        Arc::new(container),
    )
}

fn req(method: Method, path: &str, container: &Arc<quench_http::di::Container>) -> Request {
    Request::new(
        method,
        path.parse::<Uri>().unwrap(),
        HeaderMap::new(),
        quench_http::body::InboundBody::from_bytes(Bytes::new()),
        container.clone(),
    )
}

fn form_req(
    method: Method,
    path: &str,
    pairs: &[(&str, &str)],
    container: &Arc<quench_http::di::Container>,
) -> Request {
    let encoded = serde_urlencoded::to_string(pairs).unwrap();
    let mut headers = HeaderMap::new();
    headers.insert(
        "content-type",
        "application/x-www-form-urlencoded".parse().unwrap(),
    );
    Request::new(
        method,
        path.parse::<Uri>().unwrap(),
        headers,
        quench_http::body::InboundBody::from_bytes(Bytes::from(encoded)),
        container.clone(),
    )
}

async fn body_text(resp: quench_http::response::Response) -> String {
    let collected = resp.into_hyper().into_body().collect().await.expect("body");
    String::from_utf8_lossy(&collected.to_bytes()).into_owned()
}

async fn json_body(resp: quench_http::response::Response) -> serde_json::Value {
    let collected = resp.into_hyper().into_body().collect().await.expect("body");
    serde_json::from_slice(&collected.to_bytes()).expect("valid json body")
}

/// Splits a response into its headers and text body, since `Response` only
/// exposes headers via the consuming `into_hyper()` - a test that needs both
/// takes ownership once here instead of trying to inspect headers on a
/// borrow and then separately consume the body.
async fn parts(resp: quench_http::response::Response) -> (http::HeaderMap, String) {
    let (parts, body) = resp.into_hyper().into_parts();
    let collected = body.collect().await.expect("body");
    (
        parts.headers,
        String::from_utf8_lossy(&collected.to_bytes()).into_owned(),
    )
}

// ---------------------------------------------------------------------------
// send_message
// ---------------------------------------------------------------------------

#[tokio::test]
async fn send_message_is_unauthorized_without_a_session_when_auth_is_enabled() {
    let (app, container) = app(ContainerBuilder::new()
        .provide(auth_enabled())
        .provide(state())
        .provide(db()))
    .await;

    let resp = app
        .call(form_req(
            Method::POST,
            "/ui/chat/send",
            &[
                ("instance_id", "i1"),
                ("message", "hi"),
                ("conversation_id", "c1"),
            ],
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn send_message_renders_a_thinking_block_and_registers_the_pending_message() {
    let (app, container) = app(ContainerBuilder::new()
        .provide(auth_disabled())
        .provide(state())
        .provide(db()))
    .await;

    let resp = app
        .call(form_req(
            Method::POST,
            "/ui/chat/send",
            &[
                ("instance_id", "i1"),
                ("message", "  hello there  "),
                ("conversation_id", "c1"),
            ],
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);

    let html = body_text(resp).await;
    assert!(html.contains("message-user"));
    assert!(html.contains("message-ai"));
    assert!(html.contains("hello there"));
    assert!(html.contains("sse-connect"));
}

#[tokio::test]
async fn send_message_with_skip_user_message_only_renders_the_regenerating_block() {
    let (app, container) = app(ContainerBuilder::new()
        .provide(auth_disabled())
        .provide(state())
        .provide(db()))
    .await;

    let resp = app
        .call(form_req(
            Method::POST,
            "/ui/chat/send",
            &[
                ("instance_id", "i1"),
                ("message", "hi"),
                ("conversation_id", "c1"),
                ("skip_user_message", "true"),
            ],
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);

    let html = body_text(resp).await;
    assert!(html.contains("ui_chat_regenerating"));
    assert!(!html.contains("message-user"));
}

// ---------------------------------------------------------------------------
// delete_modal / delete_modal_empty / delete_conversation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn delete_modal_empty_renders_a_closed_shell() {
    let (app, container) = app(ContainerBuilder::new().provide(db())).await;
    let resp = app
        .call(req(
            Method::GET,
            "/ui/chat/conversations/delete-modal/empty",
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let html = body_text(resp).await;
    assert!(html.contains("confirm-delete-modal"));
    assert!(!html.contains("open"));
}

#[tokio::test]
async fn delete_modal_names_the_conversation_when_it_exists() {
    let db = db();
    let repo = db.repository::<Conversation>();
    repo.create(&Conversation {
        id: "c1".to_string(),
        title: "My chat".to_string(),
        active_message_id: None,
        owner: "admin".to_string(),
        project_id: None,
        updated_at: "2026-01-01T00:00:00Z".to_string(),
    })
    .await
    .unwrap();

    let (app, container) = app(ContainerBuilder::new().provide(db)).await;
    let resp = app
        .call(req(
            Method::GET,
            "/ui/chat/conversations/delete-modal/c1",
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let html = body_text(resp).await;
    assert!(html.contains("My chat"));
}

#[tokio::test]
async fn delete_modal_falls_back_to_a_generic_label_for_an_unknown_conversation() {
    let (app, container) = app(ContainerBuilder::new().provide(db())).await;
    let resp = app
        .call(req(
            Method::GET,
            "/ui/chat/conversations/delete-modal/does-not-exist",
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let html = body_text(resp).await;
    assert!(html.contains("ui_chat_this_conversation"));
}

#[tokio::test]
async fn delete_conversation_redirects_home_when_it_was_the_active_conversation() {
    let db = db();
    let repo = db.repository::<Conversation>();
    repo.create(&Conversation {
        id: "c1".to_string(),
        title: "t".to_string(),
        active_message_id: None,
        owner: "admin".to_string(),
        project_id: None,
        updated_at: "2026-01-01T00:00:00Z".to_string(),
    })
    .await
    .unwrap();

    let (app, container) = app(ContainerBuilder::new().provide(db)).await;
    let resp = app
        .call(req(
            Method::POST,
            "/ui/chat/conversations/delete/c1?active_id=c1",
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let (headers, _) = parts(resp).await;
    assert!(headers.contains_key("hx-redirect"));
}

#[tokio::test]
async fn delete_conversation_returns_an_oob_removal_when_it_was_not_active() {
    let db = db();
    let repo = db.repository::<Conversation>();
    repo.create(&Conversation {
        id: "c1".to_string(),
        title: "t".to_string(),
        active_message_id: None,
        owner: "admin".to_string(),
        project_id: None,
        updated_at: "2026-01-01T00:00:00Z".to_string(),
    })
    .await
    .unwrap();

    let (app, container) = app(ContainerBuilder::new().provide(db)).await;
    let resp = app
        .call(req(
            Method::POST,
            "/ui/chat/conversations/delete/c1?active_id=other",
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let (headers, html) = parts(resp).await;
    assert!(!headers.contains_key("hx-redirect"));
    assert!(html.contains("hx-swap-oob"));
    assert!(html.contains("history-item-c1"));
}

// ---------------------------------------------------------------------------
// switch_branch / switch_active_message / get_siblings / get_conversation_messages
// ---------------------------------------------------------------------------

async fn seed_thread(db: &Db) {
    let repo = db.repository::<Message>();
    for message in [
        Message {
            id: "root".to_string(),
            conversation_id: "conv".to_string(),
            parent_id: None,
            role: "user".to_string(),
            content: "q".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
        },
        Message {
            id: "a".to_string(),
            conversation_id: "conv".to_string(),
            parent_id: Some("root".to_string()),
            role: "assistant".to_string(),
            content: "a".to_string(),
            created_at: "2026-01-01T00:00:01Z".to_string(),
        },
        Message {
            id: "a-child".to_string(),
            conversation_id: "conv".to_string(),
            parent_id: Some("a".to_string()),
            role: "user".to_string(),
            content: "follow up".to_string(),
            created_at: "2026-01-01T00:00:02Z".to_string(),
        },
    ] {
        repo.create(&message).await.unwrap();
    }
    db.repository::<Conversation>()
        .create(&Conversation {
            id: "conv".to_string(),
            title: "t".to_string(),
            active_message_id: Some("root".to_string()),
            owner: "admin".to_string(),
            project_id: None,
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn switch_active_message_follows_the_newest_child_chain_to_the_tip() {
    let db = db();
    seed_thread(&db).await;

    switch_active_message(&db, "conv", "root").await.unwrap();

    let conv = db
        .repository::<Conversation>()
        .read("conv")
        .await
        .unwrap()
        .unwrap();
    // root -> a -> a-child is the only chain, so it walks all the way to the leaf.
    assert_eq!(conv.active_message_id.as_deref(), Some("a-child"));
}

#[tokio::test]
async fn switch_branch_redirects_with_the_conversation_id() {
    let db = db();
    seed_thread(&db).await;

    let (app, container) = app(ContainerBuilder::new().provide(db)).await;
    let resp = app
        .call(form_req(
            Method::POST,
            "/ui/chat/conversations/switch",
            &[("conversation_id", "conv"), ("target_message_id", "a")],
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let (headers, _) = parts(resp).await;
    let location = headers
        .get("hx-redirect")
        .expect("HX-Redirect header")
        .to_str()
        .unwrap();
    assert!(location.contains("conversation_id=conv"));
}

#[tokio::test]
async fn get_siblings_returns_only_messages_sharing_the_same_parent() {
    let db = db();
    seed_thread(&db).await;

    let siblings = get_siblings(&db, "conv", Some("root")).await.unwrap();
    assert_eq!(siblings.len(), 1);
    assert_eq!(siblings[0].id, "a");

    let roots = get_siblings(&db, "conv", None).await.unwrap();
    assert_eq!(roots.len(), 1);
    assert_eq!(roots[0].id, "root");
}

#[tokio::test]
async fn get_conversation_message_nodes_walks_the_active_chain_in_order() {
    let db = db();
    seed_thread(&db).await;

    let nodes = get_conversation_message_nodes(&db, Some("a-child"))
        .await
        .unwrap();
    let ids: Vec<&str> = nodes.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, vec!["root", "a", "a-child"]);
}

#[tokio::test]
async fn get_conversation_messages_is_empty_without_an_active_message() {
    let db = db();
    let messages = get_conversation_messages(&db, None).await.unwrap();
    assert!(messages.is_empty());
}

// ---------------------------------------------------------------------------
// edit_form / handle_edit
// ---------------------------------------------------------------------------

#[tokio::test]
async fn edit_form_is_not_found_for_an_unknown_message() {
    let (app, container) = app(ContainerBuilder::new().provide(db())).await;
    let resp = app
        .call(req(Method::GET, "/ui/chat/edit-form/nope", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn edit_form_renders_a_textarea_prefilled_with_the_message_content() {
    let db = db();
    db.repository::<Message>()
        .create(&Message {
            id: "m1".to_string(),
            conversation_id: "conv".to_string(),
            parent_id: None,
            role: "user".to_string(),
            content: "original text".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
        })
        .await
        .unwrap();

    let (app, container) = app(ContainerBuilder::new().provide(db)).await;
    let resp = app
        .call(req(Method::GET, "/ui/chat/edit-form/m1", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let html = body_text(resp).await;
    assert!(html.contains("original text"));
}

#[tokio::test]
async fn handle_edit_is_not_found_for_an_unknown_message() {
    let (app, container) = app(ContainerBuilder::new().provide(db())).await;
    let resp = app
        .call(form_req(
            Method::POST,
            "/ui/chat/handle-edit",
            &[("message_id", "nope"), ("new_content", "x")],
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn handle_edit_branches_a_new_user_message_from_the_same_parent_and_redirects() {
    let db = db();
    db.repository::<Message>()
        .create(&Message {
            id: "m1".to_string(),
            conversation_id: "conv".to_string(),
            parent_id: Some("root".to_string()),
            role: "user".to_string(),
            content: "original".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
        })
        .await
        .unwrap();
    db.repository::<Conversation>()
        .create(&Conversation {
            id: "conv".to_string(),
            title: "t".to_string(),
            active_message_id: Some("m1".to_string()),
            owner: "admin".to_string(),
            project_id: None,
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        })
        .await
        .unwrap();

    let (app, container) = app(ContainerBuilder::new().provide(db.clone())).await;
    let resp = app
        .call(form_req(
            Method::POST,
            "/ui/chat/handle-edit",
            &[("message_id", "m1"), ("new_content", "  edited  ")],
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let (headers, _) = parts(resp).await;
    assert!(headers.contains_key("hx-redirect"));

    let conv = db
        .repository::<Conversation>()
        .read("conv")
        .await
        .unwrap()
        .unwrap();
    assert_ne!(conv.active_message_id.as_deref(), Some("m1"));

    let new_id = conv.active_message_id.unwrap();
    let new_msg = db
        .repository::<Message>()
        .read(&new_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(new_msg.content, "edited");
    assert_eq!(new_msg.parent_id.as_deref(), Some("root"));
}

// ---------------------------------------------------------------------------
// token_stats
// ---------------------------------------------------------------------------

#[tokio::test]
async fn token_stats_is_unauthorized_without_a_session_when_auth_is_enabled() {
    let (app, container) = app(ContainerBuilder::new()
        .provide(auth_enabled())
        .provide(db())
        .provide(sage_config()))
    .await;
    let resp = app
        .call(req(Method::GET, "/ui/chat/stats/conv", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn token_stats_reports_usage_for_an_empty_conversation() {
    let (app, container) = app(ContainerBuilder::new()
        .provide(auth_disabled())
        .provide(db())
        .provide(sage_config()))
    .await;
    let resp = app
        .call(req(Method::GET, "/ui/chat/stats/conv", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = json_body(resp).await;
    assert_eq!(body["success"], true);
}

// ---------------------------------------------------------------------------
// Pure helpers
// ---------------------------------------------------------------------------

#[test]
fn chat_request_file_id_list_trims_and_drops_empty_entries() {
    let req = ChatRequest {
        instance_id: "i".to_string(),
        message: "m".to_string(),
        conversation_id: "c".to_string(),
        project_id: None,
        search_provider: None,
        parent_id: None,
        capability_profile: None,
        tool_confirmations: Vec::new(),
        skip_user_message: false,
        file_ids: " a , , b ,c".to_string(),
    };
    assert_eq!(
        req.file_id_list(),
        vec!["a".to_string(), "b".to_string(), "c".to_string()]
    );
}

#[test]
fn chat_request_file_id_list_is_empty_for_a_blank_field() {
    let req = ChatRequest {
        instance_id: "i".to_string(),
        message: "m".to_string(),
        conversation_id: "c".to_string(),
        project_id: None,
        search_provider: None,
        parent_id: None,
        capability_profile: None,
        tool_confirmations: Vec::new(),
        skip_user_message: false,
        file_ids: String::new(),
    };
    assert!(req.file_id_list().is_empty());
}
