//! The conversation, project, model and turn-start routes the browser client calls, and the tree helpers under them.

use crate::env_support::env_lock;
use crate::mock_estate::{CHAT_MODEL, mock_estate};
use quench_auth::domain::jwt::JwtConfig;
use quench_core::bytes::Bytes;
use quench_db::prelude::{Crud, Db};
use quench_http::di::ContainerBuilder;
use quench_http::endpoint::Endpoint;
use quench_http::http::{HeaderMap, Method, StatusCode, Uri};
use quench_http::request::Request;
use sage_core::{
    ConversationItem, ConversationView, Created, ModelsView, Project, Resumed, TurnStarted,
};
use sage_service::domain::models::{Conversation, Message};
use sage_service::routers::conversations::*;
use sage_service::routers::turns::{ChatRequest, ChatState};
use std::sync::Arc;

fn db() -> Db {
    Db::InMemory(quench_db::InMemoryDb::new())
}

fn state() -> Arc<ChatState> {
    Arc::new(ChatState {
        pending_messages: quench_core::map::ShardedMap::new(),
    })
}

fn sage_config() -> sage_service::config::SageConfig {
    sage_service::config::SageConfig {
        system_prompt: "you are sage".to_string(),
        default_models: vec![sage_service::config::DefaultModel {
            name: CHAT_MODEL.to_string(),
            gpu_memory_utilization: None,
            max_model_len: None,
            quantization: None,
            dtype: None,
            limit_mm_per_prompt: None,
            max_num_seqs: None,
            enable_tool_calling: false,
            task: None,
            device: None,
        }],
        supported_models: vec!["*".to_string()],
        default_search_provider: "duckduckgo".to_string(),
        available_search_providers: vec!["duckduckgo".to_string(), "brave".to_string()],
        capability_profile: sage_service::tools::capabilities::get_profile("web_assistant")
            .expect("web_assistant profile exists"),
        stop_models_on_shutdown: false,
    }
}

struct Rig {
    app: Arc<dyn Endpoint>,
    container: Arc<quench_http::di::Container>,
    db: Db,
    state: Arc<ChatState>,
}

async fn rig_with(jwt: JwtConfig, builder: ContainerBuilder) -> Rig {
    sage_service::routers::register_routes();
    let db = db();
    let state = state();
    let container = builder
        .provide(jwt)
        .provide(db.clone())
        .provide_arc(state.clone())
        .provide(sage_config())
        .build()
        .await
        .unwrap();
    Rig {
        app: quench_starter::http::discover_and_mount("/"),
        container: Arc::new(container),
        db,
        state,
    }
}

async fn rig() -> Rig {
    rig_with(JwtConfig::for_tests(), ContainerBuilder::new()).await
}

fn request(method: Method, path: &str, body: Option<&str>, rig: &Rig) -> Request {
    let mut headers = HeaderMap::new();
    if body.is_some() {
        headers.insert("content-type", "application/json".parse().unwrap());
    }
    Request::new(
        method,
        path.parse::<Uri>().unwrap(),
        headers,
        quench_http::body::InboundBody::from_bytes(Bytes::from(body.unwrap_or("").to_string())),
        rig.container.clone(),
    )
}

async fn call(rig: &Rig, method: Method, path: &str, body: Option<&str>) -> (StatusCode, String) {
    let resp = rig.app.call(request(method, path, body, rig)).await;
    let parts = resp.into_inner();
    let status = parts.status;
    let collected = parts.body.collect_limited(usize::MAX).await.expect("body");
    (status, String::from_utf8_lossy(&collected).into_owned())
}

async fn get_json<T: quench_serde::Deserialize>(rig: &Rig, path: &str) -> T {
    let (status, body) = call(rig, Method::GET, path, None).await;
    assert_eq!(status, StatusCode::OK, "{path}: {body}");
    quench_serde::json::from_str(&body).unwrap_or_else(|e| panic!("{path}: {e}: {body}"))
}

fn message(id: &str, parent: Option<&str>, role: &str, content: &str, second: u32) -> Message {
    Message {
        id: id.to_string(),
        conversation_id: "conv".to_string(),
        parent_id: parent.map(str::to_string),
        role: role.to_string(),
        content: content.to_string(),
        created_at: format!("2026-01-01T00:00:{second:02}Z"),
    }
}

fn conversation(id: &str, owner: &str, active: Option<&str>, updated: &str) -> Conversation {
    Conversation {
        id: id.to_string(),
        title: format!("title {id}"),
        active_message_id: active.map(str::to_string),
        owner: owner.to_string(),
        project_id: None,
        updated_at: updated.to_string(),
    }
}

/// `root` (user) with two answers `a` and `b`, and `b` continued by `follow`; the tip is `follow`.
async fn seed_thread(db: &Db) {
    let repo = db.repository::<Message>();
    for m in [
        message("root", None, "user", "q", 0),
        message("a", Some("root"), "assistant", "answer a", 1),
        message("b", Some("root"), "assistant", "answer b", 2),
        message("follow", Some("b"), "user", "follow up", 3),
    ] {
        repo.create(&m).await.unwrap();
    }
    db.repository::<Conversation>()
        .create(&conversation(
            "conv",
            "admin",
            Some("follow"),
            "2026-01-01T00:00:03Z",
        ))
        .await
        .unwrap();
}

// ---------------------------------------------------------------------------
// the tree helpers
// ---------------------------------------------------------------------------

#[quench_test::test]
async fn retrieves_only_the_selected_conversation_branch() {
    let db = db();
    seed_thread(&db).await;
    let branch = get_conversation_messages(&db, Some("b")).await.unwrap();
    let texts: Vec<&str> = branch.iter().map(|m| m.content.as_str()).collect();
    assert_eq!(texts, vec!["q", "answer b"]);
    let siblings = get_siblings(&db, "conv", Some("root")).await.unwrap();
    let ids: Vec<&str> = siblings.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, vec!["a", "b"]);
    let roots = get_siblings(&db, "conv", None).await.unwrap();
    assert_eq!(roots.len(), 1);
}

#[quench_test::test]
async fn the_active_chain_is_walked_in_order_and_empty_without_a_tip() {
    let db = db();
    seed_thread(&db).await;
    let nodes = get_conversation_message_nodes(&db, Some("follow"))
        .await
        .unwrap();
    let ids: Vec<&str> = nodes.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, vec!["root", "b", "follow"]);
    assert!(
        get_conversation_messages(&db, None)
            .await
            .unwrap()
            .is_empty()
    );
}

#[quench_test::test]
async fn switching_follows_the_newest_child_chain_to_the_tip() {
    let db = db();
    seed_thread(&db).await;
    switch_active_message(&db, "conv", "a").await.unwrap();
    let conv = db
        .repository::<Conversation>()
        .read("conv")
        .await
        .unwrap()
        .unwrap();
    // `a` has no children: it is the tip
    assert_eq!(conv.active_message_id.as_deref(), Some("a"));
    switch_active_message(&db, "conv", "root").await.unwrap();
    let conv = db
        .repository::<Conversation>()
        .read("conv")
        .await
        .unwrap()
        .unwrap();
    // the newest answer of root is b, continued by follow
    assert_eq!(conv.active_message_id.as_deref(), Some("follow"));
}

// ---------------------------------------------------------------------------
// the routes
// ---------------------------------------------------------------------------

#[quench_test::test]
async fn every_route_needs_a_session_when_auth_is_enabled() {
    let mut jwt = JwtConfig::for_tests();
    jwt.auth_enabled = true;
    let rig = rig_with(
        jwt,
        ContainerBuilder::new()
            .provide(sage_service::clients::switchboard::SwitchboardClient::new()),
    )
    .await;
    let edit = r#"{"new_content":"x"}"#;
    let switch = r#"{"target_message_id":"a"}"#;
    let regenerate = r#"{"message_id":"a"}"#;
    let project = r#"{"name":"x"}"#;
    let turn = r#"{"instance_id":"i","message":"x","conversation_id":"c"}"#;
    for (method, path, body) in [
        (Method::GET, "/api/v1/conversations", None),
        (Method::GET, "/api/v1/conversations/c", None),
        (Method::DELETE, "/api/v1/conversations/c", None),
        (Method::POST, "/api/v1/conversations/c/switch", Some(switch)),
        (Method::POST, "/api/v1/conversations/c/resume", None),
        (Method::GET, "/api/v1/projects", None),
        (Method::POST, "/api/v1/projects", Some(project)),
        (Method::GET, "/api/v1/conversation-id", None),
        (Method::POST, "/api/v1/messages/m/edit", Some(edit)),
        (Method::POST, "/api/v1/turns", Some(turn)),
        (Method::POST, "/api/v1/turns/regenerate", Some(regenerate)),
        (Method::GET, "/api/v1/models", None),
    ] {
        let (status, _) = call(&rig, method.clone(), path, body).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {path}");
    }
}

#[quench_test::test]
async fn the_list_holds_only_the_callers_conversations_newest_first() {
    let rig = rig().await;
    let repo = rig.db.repository::<Conversation>();
    repo.create(&conversation("old", "admin", None, "2026-01-01T00:00:00Z"))
        .await
        .unwrap();
    repo.create(&conversation("new", "admin", None, "2026-02-01T00:00:00Z"))
        .await
        .unwrap();
    repo.create(&conversation(
        "theirs",
        "mallory",
        None,
        "2026-03-01T00:00:00Z",
    ))
    .await
    .unwrap();
    let list: Vec<ConversationItem> = get_json(&rig, "/api/v1/conversations").await;
    let ids: Vec<&str> = list.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids, vec!["new", "old"]);
    assert_eq!(list[0].title, "title new");
}

#[quench_test::test]
async fn a_conversation_comes_with_its_alternatives_and_only_to_its_owner() {
    let rig = rig().await;
    seed_thread(&rig.db).await;
    let view: ConversationView = get_json(&rig, "/api/v1/conversations/conv").await;
    let ids: Vec<&str> = view.messages.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, vec!["root", "b", "follow"]);
    let answer = &view.messages[1];
    assert_eq!(answer.siblings, vec!["a".to_string(), "b".to_string()]);
    assert_eq!(answer.position(), (2, 2));
    assert_eq!(answer.neighbours(), Some(("a", "a")));
    assert_eq!(view.messages[0].siblings, vec!["root".to_string()]);

    rig.db
        .repository::<Conversation>()
        .create(&conversation(
            "theirs",
            "mallory",
            None,
            "2026-01-01T00:00:00Z",
        ))
        .await
        .unwrap();
    let (status, _) = call(&rig, Method::GET, "/api/v1/conversations/theirs", None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, body) = call(&rig, Method::GET, "/api/v1/conversations/none", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body.contains("api_error_conversation_not_found"), "{body}");
}

#[quench_test::test]
async fn deleting_removes_the_conversation_and_refuses_another_persons() {
    let rig = rig().await;
    let repo = rig.db.repository::<Conversation>();
    repo.create(&conversation("mine", "admin", None, "2026-01-01T00:00:00Z"))
        .await
        .unwrap();
    repo.create(&conversation(
        "theirs",
        "mallory",
        None,
        "2026-01-01T00:00:00Z",
    ))
    .await
    .unwrap();
    let (status, _) = call(&rig, Method::DELETE, "/api/v1/conversations/mine", None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(repo.read("mine").await.unwrap().is_none());
    let (status, _) = call(&rig, Method::DELETE, "/api/v1/conversations/mine", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = call(&rig, Method::DELETE, "/api/v1/conversations/theirs", None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(repo.read("theirs").await.unwrap().is_some());
}

#[quench_test::test]
async fn switching_answers_moves_the_tip_and_only_within_the_conversation() {
    let rig = rig().await;
    seed_thread(&rig.db).await;
    let (status, _) = call(
        &rig,
        Method::POST,
        "/api/v1/conversations/conv/switch",
        Some(r#"{"target_message_id":"a"}"#),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let conv = rig
        .db
        .repository::<Conversation>()
        .read("conv")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(conv.active_message_id.as_deref(), Some("a"));

    let (status, _) = call(
        &rig,
        Method::POST,
        "/api/v1/conversations/conv/switch",
        Some(r#"{"target_message_id":"nope"}"#),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = call(
        &rig,
        Method::POST,
        "/api/v1/conversations/missing/switch",
        Some(r#"{"target_message_id":"a"}"#),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[quench_test::test]
async fn editing_branches_a_new_user_message_from_the_same_parent() {
    let rig = rig().await;
    seed_thread(&rig.db).await;
    let (status, _) = call(
        &rig,
        Method::POST,
        "/api/v1/messages/none/edit",
        Some(r#"{"new_content":"x"}"#),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = call(
        &rig,
        Method::POST,
        "/api/v1/messages/a/edit",
        Some(r#"{"new_content":"x"}"#),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "only the person's own messages"
    );
    let (status, _) = call(
        &rig,
        Method::POST,
        "/api/v1/messages/follow/edit",
        Some(r#"{"new_content":"   "}"#),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, body) = call(
        &rig,
        Method::POST,
        "/api/v1/messages/follow/edit",
        Some(r#"{"new_content":"  a better question "}"#),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let created: Created = quench_serde::json::from_str(&body).unwrap();
    let edited = rig
        .db
        .repository::<Message>()
        .read(&created.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(edited.content, "a better question");
    assert_eq!(edited.parent_id.as_deref(), Some("b"));
    assert_eq!(edited.role, "user");
    let conv = rig
        .db
        .repository::<Conversation>()
        .read("conv")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(conv.active_message_id.as_deref(), Some(created.id.as_str()));
    // the original is still there as an alternative
    let siblings = get_siblings(&rig.db, "conv", Some("b")).await.unwrap();
    assert_eq!(siblings.len(), 2);
}

#[quench_test::test]
async fn projects_are_listed_by_owner_and_created_with_a_name() {
    let rig = rig().await;
    let repo = rig.db.repository::<sage_service::domain::models::Project>();
    for (id, name, owner, at) in [
        ("p1", "Atlas", "admin", "2026-01-02T00:00:00Z"),
        ("p2", "Borealis", "admin", "2026-01-01T00:00:00Z"),
        ("p3", "Someone else's", "mallory", "2026-01-03T00:00:00Z"),
    ] {
        repo.create(&sage_service::domain::models::Project {
            id: id.to_string(),
            name: name.to_string(),
            owner: owner.to_string(),
            created_at: at.to_string(),
            updated_at: at.to_string(),
        })
        .await
        .unwrap();
    }
    let list: Vec<Project> = get_json(&rig, "/api/v1/projects").await;
    let names: Vec<&str> = list.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, vec!["Atlas", "Borealis"]);

    let (status, body) = call(
        &rig,
        Method::POST,
        "/api/v1/projects",
        Some(r#"{"name":"  Notes "}"#),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let created: Project = quench_serde::json::from_str(&body).unwrap();
    assert_eq!(created.name, "Notes");
    let stored = repo.read(&created.id).await.unwrap().unwrap();
    assert_eq!(stored.owner, "admin");
    let (status, body) = call(
        &rig,
        Method::POST,
        "/api/v1/projects",
        Some(r#"{"name":" "}"#),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body.contains("api_error_project_name"), "{body}");
}

#[quench_test::test]
async fn a_fresh_conversation_id_is_made_without_storing_anything() {
    let rig = rig().await;
    let a: Created = get_json(&rig, "/api/v1/conversation-id").await;
    let b: Created = get_json(&rig, "/api/v1/conversation-id").await;
    assert_ne!(a.id, b.id);
    assert_eq!(a.id.len(), 36);
    assert!(
        rig.db
            .repository::<Conversation>()
            .list()
            .await
            .unwrap()
            .is_empty()
    );
}

#[quench_test::test]
async fn starting_a_turn_stores_a_trimmed_request_and_refuses_empty_or_foreign_ones() {
    let rig = rig().await;
    let body = r#"{"instance_id":"i1","message":"  hello  ","conversation_id":"c1","project_id":null,"search_provider":"brave","file_ids":"f1,f2"}"#;
    let (status, text) = call(&rig, Method::POST, "/api/v1/turns", Some(body)).await;
    assert_eq!(status, StatusCode::OK, "{text}");
    let started: TurnStarted = quench_serde::json::from_str(&text).unwrap();
    let pending = rig
        .state
        .pending_messages
        .get(&started.id)
        .expect("pending");
    assert_eq!(pending.message, "hello");
    assert_eq!(pending.instance_id, "i1");
    assert_eq!(pending.search_provider.as_deref(), Some("brave"));
    assert_eq!(
        pending.file_id_list(),
        vec!["f1".to_string(), "f2".to_string()]
    );
    assert!(!pending.skip_user_message);

    // a client cannot ask for a regeneration through the send route
    let sneaky = r#"{"instance_id":"i1","message":"x","conversation_id":"c1","skip_user_message":true,"parent_id":"p"}"#;
    let (_, text) = call(&rig, Method::POST, "/api/v1/turns", Some(sneaky)).await;
    let started: TurnStarted = quench_serde::json::from_str(&text).unwrap();
    let pending = rig.state.pending_messages.get(&started.id).unwrap();
    assert!(!pending.skip_user_message);
    assert_eq!(pending.parent_id, None);

    let empty = r#"{"instance_id":"i1","message":"  ","conversation_id":"c1"}"#;
    let (status, text) = call(&rig, Method::POST, "/api/v1/turns", Some(empty)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(text.contains("api_error_empty_message"), "{text}");

    rig.db
        .repository::<Conversation>()
        .create(&conversation(
            "theirs",
            "mallory",
            None,
            "2026-01-01T00:00:00Z",
        ))
        .await
        .unwrap();
    let foreign = r#"{"instance_id":"i1","message":"x","conversation_id":"theirs"}"#;
    let (status, _) = call(&rig, Method::POST, "/api/v1/turns", Some(foreign)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[allow(clippy::await_holding_lock)] // the mock estate sets process-wide env vars
#[quench_test::test]
async fn opening_a_conversation_that_ends_with_the_persons_message_asks_for_an_answer() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (_server, switchboard) = mock_estate(&[]).await;
    let rig = rig_with(
        JwtConfig::for_tests(),
        ContainerBuilder::new().provide(switchboard),
    )
    .await;
    seed_thread(&rig.db).await;

    // the tip is the user's "follow up"
    let (status, text) = call(
        &rig,
        Method::POST,
        "/api/v1/conversations/conv/resume",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{text}");
    let resumed: Resumed = quench_serde::json::from_str(&text).unwrap();
    let id = resumed.id.expect("a turn to answer it");
    let pending: ChatRequest = rig.state.pending_messages.get(&id).unwrap();
    assert!(pending.skip_user_message);
    assert_eq!(pending.message, "follow up");
    assert_eq!(pending.parent_id.as_deref(), Some("follow"));
    assert_eq!(pending.instance_id, format!("id-{CHAT_MODEL}"));

    // after Sage's answer there is nothing to do
    switch_active_message(&rig.db, "conv", "a").await.unwrap();
    let (_, text) = call(
        &rig,
        Method::POST,
        "/api/v1/conversations/conv/resume",
        None,
    )
    .await;
    assert_eq!(
        quench_serde::json::from_str::<Resumed>(&text).unwrap().id,
        None
    );
    let (status, _) = call(
        &rig,
        Method::POST,
        "/api/v1/conversations/nope/resume",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[allow(clippy::await_holding_lock)] // the mock estate sets process-wide env vars
#[quench_test::test]
async fn the_models_say_whether_the_chat_can_be_used_and_what_the_composer_offers() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (_server, switchboard) = mock_estate(&[]).await;
    let rig = rig_with(
        JwtConfig::for_tests(),
        ContainerBuilder::new().provide(switchboard),
    )
    .await;
    let view: ModelsView = get_json(&rig, "/api/v1/models").await;
    assert!(view.switchboard_ok);
    assert!(view.ready, "the one default model runs: {view:?}");
    // only chat-capable instances are offered; the embedding model is not
    let models: Vec<&str> = view.instances.iter().map(|i| i.model.as_str()).collect();
    assert_eq!(models, vec![CHAT_MODEL]);
    assert_eq!(view.defaults.len(), 1);
    assert_eq!(view.defaults[0].state, "running");
    assert_eq!(view.default_search_provider, "duckduckgo");
    let labels: Vec<&str> = view
        .search_providers
        .iter()
        .map(|p| p.label.as_str())
        .collect();
    assert_eq!(labels, vec!["DuckDuckGo", "Brave Search"]);
    assert!(
        view.accept.contains(".pdf") && view.accept.contains(".rs"),
        "{}",
        view.accept
    );
}

#[allow(clippy::await_holding_lock)] // process-wide env vars
#[quench_test::test]
async fn with_switchboard_down_the_models_are_unknown_and_not_ready() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    quench_core::env::set("GATEHOUSE_URL", "http://127.0.0.1:1");
    quench_core::env::set("SWITCHBOARD_URL", "http://127.0.0.1:1");
    let rig = rig_with(
        JwtConfig::for_tests(),
        ContainerBuilder::new()
            .provide(sage_service::clients::switchboard::SwitchboardClient::new()),
    )
    .await;
    let view: ModelsView = get_json(&rig, "/api/v1/models").await;
    assert!(!view.switchboard_ok);
    assert!(!view.ready);
    assert!(view.instances.is_empty());
    assert_eq!(view.defaults[0].state, "unknown");
}

#[quench_test::test]
async fn the_token_stats_route_still_answers() {
    let rig = rig().await;
    let (status, body) = call(&rig, Method::GET, "/ui/chat/stats/conv", None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.contains("\"success\":true"), "{body}");
}

#[quench_test::test]
fn the_staged_file_ids_are_split_trimmed_and_blanks_dropped() {
    let mut req = ChatRequest {
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
    assert_eq!(req.file_id_list(), vec!["a", "b", "c"]);
    req.file_ids = String::new();
    assert!(req.file_id_list().is_empty());
}
