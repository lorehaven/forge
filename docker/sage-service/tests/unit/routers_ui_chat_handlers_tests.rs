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

use actix_web::test as actix_test;
use actix_web::web::Data;
use actix_web::{App, http::StatusCode};
use quench_auth::prelude::JwtConfig;
use quench_db::prelude::{Crud, Db};
use sage_service::domain::models::{Conversation, Message};
use sage_service::routers::ui::chat::*;

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

// ---------------------------------------------------------------------------
// send_message
// ---------------------------------------------------------------------------

#[actix_test]
async fn send_message_is_unauthorized_without_a_session_when_auth_is_enabled() {
    let app = actix_test::init_service(
        App::new()
            .app_data(Data::new(auth_enabled()))
            .app_data(Data::new(state()))
            .app_data(Data::new(db()))
            .service(send_message),
    )
    .await;

    let req = actix_test::TestRequest::post()
        .uri("/send")
        .set_form([
            ("instance_id", "i1"),
            ("message", "hi"),
            ("conversation_id", "c1"),
        ])
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[actix_test]
async fn send_message_renders_a_thinking_block_and_registers_the_pending_message() {
    let chat_state = state();
    let app = actix_test::init_service(
        App::new()
            .app_data(Data::new(auth_disabled()))
            .app_data(Data::new(chat_state))
            .app_data(Data::new(db()))
            .service(send_message),
    )
    .await;

    let req = actix_test::TestRequest::post()
        .uri("/send")
        .set_form([
            ("instance_id", "i1"),
            ("message", "  hello there  "),
            ("conversation_id", "c1"),
        ])
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body = actix_test::read_body(resp).await;
    let html = String::from_utf8(body.to_vec()).expect("utf8");
    assert!(html.contains("message-user"));
    assert!(html.contains("message-ai"));
    assert!(html.contains("hello there"));
    assert!(html.contains("sse-connect"));
}

#[actix_test]
async fn send_message_with_skip_user_message_only_renders_the_regenerating_block() {
    let app = actix_test::init_service(
        App::new()
            .app_data(Data::new(auth_disabled()))
            .app_data(Data::new(state()))
            .app_data(Data::new(db()))
            .service(send_message),
    )
    .await;

    let req = actix_test::TestRequest::post()
        .uri("/send")
        .set_form([
            ("instance_id", "i1"),
            ("message", "hi"),
            ("conversation_id", "c1"),
            ("skip_user_message", "true"),
        ])
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body = actix_test::read_body(resp).await;
    let html = String::from_utf8(body.to_vec()).expect("utf8");
    assert!(html.contains("ui_chat_regenerating"));
    assert!(!html.contains("message-user"));
}

// ---------------------------------------------------------------------------
// delete_modal / delete_modal_empty / delete_conversation
// ---------------------------------------------------------------------------

#[actix_test]
async fn delete_modal_empty_renders_a_closed_shell() {
    let app = actix_test::init_service(App::new().service(delete_modal_empty)).await;
    let req = actix_test::TestRequest::get()
        .uri("/conversations/delete-modal/empty")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = actix_test::read_body(resp).await;
    let html = String::from_utf8(body.to_vec()).expect("utf8");
    assert!(html.contains("confirm-delete-modal"));
    assert!(!html.contains("open"));
}

#[actix_test]
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

    let app =
        actix_test::init_service(App::new().app_data(Data::new(db)).service(delete_modal)).await;
    let req = actix_test::TestRequest::get()
        .uri("/conversations/delete-modal/c1")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = actix_test::read_body(resp).await;
    let html = String::from_utf8(body.to_vec()).expect("utf8");
    assert!(html.contains("My chat"));
}

#[actix_test]
async fn delete_modal_falls_back_to_a_generic_label_for_an_unknown_conversation() {
    let app =
        actix_test::init_service(App::new().app_data(Data::new(db())).service(delete_modal)).await;
    let req = actix_test::TestRequest::get()
        .uri("/conversations/delete-modal/does-not-exist")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = actix_test::read_body(resp).await;
    let html = String::from_utf8(body.to_vec()).expect("utf8");
    assert!(html.contains("ui_chat_this_conversation"));
}

#[actix_test]
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

    let app = actix_test::init_service(
        App::new()
            .app_data(Data::new(db))
            .service(delete_conversation),
    )
    .await;
    let req = actix_test::TestRequest::post()
        .uri("/conversations/delete/c1?active_id=c1")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(resp.headers().contains_key("HX-Redirect"));
}

#[actix_test]
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

    let app = actix_test::init_service(
        App::new()
            .app_data(Data::new(db))
            .service(delete_conversation),
    )
    .await;
    let req = actix_test::TestRequest::post()
        .uri("/conversations/delete/c1?active_id=other")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(!resp.headers().contains_key("HX-Redirect"));
    let body = actix_test::read_body(resp).await;
    let html = String::from_utf8(body.to_vec()).expect("utf8");
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

#[actix_test]
async fn switch_branch_redirects_with_the_conversation_id() {
    let db = db();
    seed_thread(&db).await;

    let app =
        actix_test::init_service(App::new().app_data(Data::new(db)).service(switch_branch)).await;
    let req = actix_test::TestRequest::post()
        .uri("/conversations/switch")
        .set_form([("conversation_id", "conv"), ("target_message_id", "a")])
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let location = resp.headers().get("HX-Redirect").unwrap().to_str().unwrap();
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

#[actix_test]
async fn edit_form_is_not_found_for_an_unknown_message() {
    let app =
        actix_test::init_service(App::new().app_data(Data::new(db())).service(edit_form)).await;
    let req = actix_test::TestRequest::get()
        .uri("/edit-form/nope")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[actix_test]
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

    let app = actix_test::init_service(App::new().app_data(Data::new(db)).service(edit_form)).await;
    let req = actix_test::TestRequest::get()
        .uri("/edit-form/m1")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = actix_test::read_body(resp).await;
    let html = String::from_utf8(body.to_vec()).expect("utf8");
    assert!(html.contains("original text"));
}

#[actix_test]
async fn handle_edit_is_not_found_for_an_unknown_message() {
    let app =
        actix_test::init_service(App::new().app_data(Data::new(db())).service(handle_edit)).await;
    let req = actix_test::TestRequest::post()
        .uri("/handle-edit")
        .set_form([("message_id", "nope"), ("new_content", "x")])
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[actix_test]
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

    let app = actix_test::init_service(
        App::new()
            .app_data(Data::new(db.clone()))
            .service(handle_edit),
    )
    .await;
    let req = actix_test::TestRequest::post()
        .uri("/handle-edit")
        .set_form([("message_id", "m1"), ("new_content", "  edited  ")])
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(resp.headers().contains_key("HX-Redirect"));

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

#[actix_test]
async fn token_stats_is_unauthorized_without_a_session_when_auth_is_enabled() {
    let app = actix_test::init_service(
        App::new()
            .app_data(Data::new(auth_enabled()))
            .app_data(Data::new(db()))
            .app_data(Data::new(sage_config()))
            .service(token_stats),
    )
    .await;
    let req = actix_test::TestRequest::get()
        .uri("/stats/conv")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[actix_test]
async fn token_stats_reports_usage_for_an_empty_conversation() {
    let app = actix_test::init_service(
        App::new()
            .app_data(Data::new(auth_disabled()))
            .app_data(Data::new(db()))
            .app_data(Data::new(sage_config()))
            .service(token_stats),
    )
    .await;
    let req = actix_test::TestRequest::get()
        .uri("/stats/conv")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = actix_test::read_body_json(resp).await;
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
