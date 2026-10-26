//! `stream_message` and `regenerate` end to end: the real `SwitchboardClient` / `VllmClient` talk
//! to one mock server (see `mock_estate`), the database is in memory, and the SSE body the browser
//! would receive is read back and checked together with what the turn stored.

use crate::env_support::env_lock;
use crate::mock_estate::{CHAT_MODEL, EMBEDDING_MODEL, completion, mock_estate, on_chat, sse};
use quench_auth::domain::jwt::JwtConfig;
use quench_core::bytes::Bytes;
use quench_db::prelude::{Crud, Db};
use quench_http::di::ContainerBuilder;
use quench_http::endpoint::Endpoint;
use quench_http::http::{HeaderMap, Method, StatusCode, Uri};
use quench_http::request::Request;
use quench_test::mock::matchers::{body_string_contains, method, path};
use quench_test::mock::{Mock, MockServer, ResponseTemplate};
use sage_service::clients::switchboard::SwitchboardClient;
use sage_service::clients::vllm::VllmClient;
use sage_service::domain::models::{Conversation, Message};
use sage_service::observability::cost_tracking::CostTracker;
use sage_service::observability::metrics::MetricsCollector;
use sage_service::routers::ui::chat::{ChatRequest, ChatState, register_routes};
use sage_service::runtime::rate_limiter::RateLimiter;
use sage_service::tools::{SearchProvider, SearchProviderRegistry};
use std::sync::Arc;

const CHAT_ID: &str = "id-chat-model";

struct FakeSearch(String);

#[quench_core::async_trait]
impl SearchProvider for FakeSearch {
    fn name(&self) -> &'static str {
        "duckduckgo"
    }
    fn requires_api_key(&self) -> bool {
        false
    }
    async fn search(&self, _query: &str) -> Result<String, String> {
        Ok(self.0.clone())
    }
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

struct Rig {
    server: MockServer,
    state: Arc<ChatState>,
    db: Db,
    app: Arc<dyn Endpoint>,
    container: Arc<quench_http::di::Container>,
}

/// The app wired to a mock estate whose search provider answers with `search_results`.
async fn rig_with(
    server: MockServer,
    switchboard: SwitchboardClient,
    search_results: &str,
    jwt: JwtConfig,
) -> Rig {
    let state = Arc::new(ChatState {
        pending_messages: quench_core::map::ShardedMap::new(),
    });
    let db = Db::InMemory(quench_db::InMemoryDb::new());
    let mut registry = SearchProviderRegistry::new();
    registry.register(
        "duckduckgo".to_string(),
        Box::new(FakeSearch(search_results.to_string())),
    );
    register_routes();
    let container = ContainerBuilder::new()
        .provide(jwt)
        .provide_arc(state.clone())
        .provide(switchboard)
        .provide(VllmClient::new())
        .provide(sage_config())
        .provide(db.clone())
        .provide(registry)
        .provide_arc(Arc::new(MetricsCollector::new()))
        .provide(tokio::sync::Mutex::new(RateLimiter::new()))
        .provide_arc(Arc::new(CostTracker::new()))
        .build()
        .await
        .expect("a container");
    Rig {
        server,
        state,
        db,
        app: quench_starter::http::discover_and_mount("/"),
        container: Arc::new(container),
    }
}

async fn rig() -> Rig {
    let (server, switchboard) = mock_estate(&[]).await;
    rig_with(server, switchboard, "no results", JwtConfig::for_tests()).await
}

fn request(instance_id: &str, message: &str, conversation_id: &str) -> ChatRequest {
    ChatRequest {
        instance_id: instance_id.to_string(),
        message: message.to_string(),
        conversation_id: conversation_id.to_string(),
        project_id: None,
        search_provider: None,
        parent_id: None,
        capability_profile: None,
        tool_confirmations: Vec::new(),
        skip_user_message: false,
        file_ids: String::new(),
    }
}

fn get(path: &str, container: &Arc<quench_http::di::Container>) -> Request {
    Request::new(
        Method::GET,
        path.parse::<Uri>().unwrap(),
        HeaderMap::new(),
        quench_http::body::InboundBody::from_bytes(Bytes::new()),
        container.clone(),
    )
}

fn form(
    path: &str,
    pairs: &[(&str, &str)],
    container: &Arc<quench_http::di::Container>,
) -> Request {
    let mut headers = HeaderMap::new();
    headers.insert(
        "content-type",
        "application/x-www-form-urlencoded".parse().unwrap(),
    );
    Request::new(
        Method::POST,
        path.parse::<Uri>().unwrap(),
        headers,
        quench_http::body::InboundBody::from_bytes(Bytes::from(
            quench_serde::urlencoded::to_string(pairs).unwrap(),
        )),
        container.clone(),
    )
}

async fn parts(resp: quench_http::response::Response) -> (HeaderMap, String) {
    let parts = resp.into_inner();
    let collected = parts.body.collect_limited(usize::MAX).await.expect("body");
    (
        parts.headers,
        String::from_utf8_lossy(&collected).into_owned(),
    )
}

/// Registers `req` as pending and streams it; returns the status and the whole SSE body.
async fn stream(rig: &Rig, id: &str, req: ChatRequest) -> (StatusCode, String) {
    rig.state.pending_messages.insert(id.to_string(), req);
    stream_pending(rig, id).await
}

async fn stream_pending(rig: &Rig, id: &str) -> (StatusCode, String) {
    let resp = rig
        .app
        .call(get(&format!("/ui/chat/stream/{id}"), &rig.container))
        .await;
    let status = resp.status();
    (status, parts(resp).await.1)
}

/// Streamed answer requests the mock received (the tool summaries are streamed too, so those
/// are told apart by their prompt).
async fn streamed_requests(server: &MockServer, summaries: bool) -> Vec<quench_serde::Value> {
    server
        .received_requests()
        .await
        .expect("recorded")
        .iter()
        .filter(|r| r.path == "/v1/chat/completions")
        .filter_map(quench_test::mock::Request::body_json)
        .filter(|b| b["stream"] == true)
        .filter(|b| b.to_string().contains("Provide the information clearly") == summaries)
        .collect()
}

async fn messages(db: &Db) -> Vec<Message> {
    let mut all = db.repository::<Message>().list().await.unwrap();
    all.sort_by(|a, b| a.created_at.cmp(&b.created_at));
    all
}

/// Environment switches a test sets for the grounded pipeline, removed again on drop.
struct GroundedEnv;

impl GroundedEnv {
    fn on() -> Self {
        quench_core::env::set("SAGE_GROUNDED", "true");
        quench_core::env::set("SAGE_GROUNDED_FETCH_PAGES", "0");
        Self
    }
}

impl Drop for GroundedEnv {
    fn drop(&mut self) {
        quench_core::env::remove("SAGE_GROUNDED");
        quench_core::env::remove("SAGE_GROUNDED_FETCH_PAGES");
    }
}

#[allow(clippy::await_holding_lock)] // env vars are process-global; held for the whole test
#[quench_test::test]
async fn an_unknown_pending_message_is_not_found_and_auth_is_required() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let rig = rig().await;
    let (status, _) = stream_pending(&rig, "nope").await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (server, switchboard) = mock_estate(&[]).await;
    let mut jwt = JwtConfig::for_tests();
    jwt.auth_enabled = true;
    let locked = rig_with(server, switchboard, "", jwt).await;
    let (status, _) = stream_pending(&locked, "nope").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[allow(clippy::await_holding_lock)]
#[quench_test::test]
async fn the_instance_must_exist_and_be_able_to_chat() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let rig = rig().await;
    let (status, body) = stream(&rig, "m1", request("missing", "hi", "c1")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body.contains("api_error_instance_not_found"), "{body}");

    let embedding = format!("id-{EMBEDDING_MODEL}");
    let (status, body) = stream(&rig, "m2", request(&embedding, "hi", "c1")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body.contains("api_error_embedding_model_chat"), "{body}");
    // a refused request stays pending, so the page can retry it
    assert!(rig.state.pending_messages.get("m1").is_some());

    // switchboard unreachable
    quench_core::env::set("GATEHOUSE_URL", "http://127.0.0.1:1");
    quench_core::env::set("SWITCHBOARD_URL", "http://127.0.0.1:1");
    let down = rig_with(
        MockServer::start().await,
        SwitchboardClient::new(),
        "",
        JwtConfig::for_tests(),
    )
    .await;
    let (status, body) = stream(&down, "m3", request(CHAT_ID, "hi", "c1")).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(body.contains("api_error_switchboard_unavailable"), "{body}");
}

#[allow(clippy::await_holding_lock)]
#[quench_test::test]
async fn a_first_message_streams_the_answer_and_stores_the_conversation() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let rig = rig().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(sse(&["Hel", "lo ", "there."]))
        .mount(&rig.server)
        .await;

    // `send_message` has already trimmed the text by the time it is pending
    let (status, body) = stream(&rig, "m1", request(CHAT_ID, "What is Rust?", "conv-1")).await;
    assert_eq!(status, StatusCode::OK);

    // growing partial answers, then the permanent message with its controls
    assert!(body.contains("event: message"), "{body}");
    assert!(body.contains("Hel"), "{body}");
    assert!(body.contains("Hello there."), "{body}");
    assert!(body.contains("hx-swap-oob=\"outerHTML:#ai-m1\""), "{body}");
    assert!(body.contains("regenerate-btn"), "{body}");
    assert!(
        body.contains("hx-swap-oob=\"outerHTML:#user-m1\""),
        "{body}"
    );
    assert!(body.contains("id=\"history-list\""), "{body}");
    assert!(body.contains("What is Rust?"), "{body}");

    let stored = messages(&rig.db).await;
    assert_eq!(stored.len(), 2);
    assert_eq!(
        (stored[0].role.as_str(), stored[0].content.as_str()),
        ("user", "What is Rust?")
    );
    assert_eq!(
        (stored[1].role.as_str(), stored[1].content.as_str()),
        ("assistant", "Hello there.")
    );
    assert_eq!(stored[1].parent_id.as_deref(), Some(stored[0].id.as_str()));

    let conv = rig
        .db
        .repository::<Conversation>()
        .read("conv-1")
        .await
        .unwrap()
        .expect("the conversation");
    assert_eq!(conv.title, "What is Rust?");
    assert_eq!(conv.owner, "admin");
    assert_eq!(
        conv.active_message_id.as_deref(),
        Some(stored[1].id.as_str())
    );
    // finished, so no longer pending
    assert!(rig.state.pending_messages.get("m1").is_none());

    // what vLLM was asked
    let sent = streamed_requests(&rig.server, false).await;
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0]["model"], CHAT_MODEL);
    let prompt = sent[0]["messages"].to_string();
    assert!(prompt.contains("you are sage"), "{prompt}");
    assert!(prompt.contains("What is Rust?"), "{prompt}");
    assert!(
        sent[0]["tools"].is_array(),
        "tools are offered outside grounded mode"
    );
}

#[allow(clippy::await_holding_lock)]
#[quench_test::test]
async fn a_follow_up_sees_the_history_and_a_regeneration_adds_a_sibling() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let rig = rig().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(sse(&["An answer."]))
        .mount(&rig.server)
        .await;

    stream(&rig, "m1", request(CHAT_ID, "first question", "conv-1")).await;
    let (_, body) = stream(&rig, "m2", request(CHAT_ID, "second question", "conv-1")).await;
    assert!(!body.contains("branch-nav"), "{body}");

    let sent = streamed_requests(&rig.server, false).await;
    let second = sent[1]["messages"].to_string();
    assert!(second.contains("first question"), "{second}");
    assert!(second.contains("An answer."), "{second}");
    assert!(second.contains("second question"), "{second}");

    let stored = messages(&rig.db).await;
    assert_eq!(stored.len(), 4);
    let answer = stored
        .iter()
        .rev()
        .find(|m| m.role == "assistant")
        .unwrap()
        .clone();
    let question = stored
        .iter()
        .find(|m| m.id == *answer.parent_id.as_ref().unwrap())
        .unwrap();
    assert_eq!(question.content, "second question");

    // regenerate: the page posts the answer's id, then streams the pending request it gets back
    let resp = rig
        .app
        .call(form(
            "/ui/chat/regenerate",
            &[("message_id", &answer.id)],
            &rig.container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let (headers, html) = parts(resp).await;
    assert_eq!(
        headers.get("HX-Retarget").unwrap().to_str().unwrap(),
        format!("#ai-{}", answer.id)
    );
    assert!(html.contains("Sage is regenerating"), "{html}");
    let pending_id = html
        .split("id=\"ai-")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .expect("the new message id")
        .to_string();
    let pending = rig
        .state
        .pending_messages
        .get(&pending_id)
        .expect("pending");
    assert!(pending.skip_user_message);
    assert_eq!(pending.message, "second question");
    assert_eq!(pending.parent_id.as_deref(), Some(question.id.as_str()));

    let (status, body) = stream_pending(&rig, &pending_id).await;
    assert_eq!(status, StatusCode::OK);
    // two answers now hang from the same question: previous/next appear, and no new user message
    assert!(body.contains("branch-nav"), "{body}");
    assert!(body.contains("2/2"), "{body}");
    assert_eq!(messages(&rig.db).await.len(), 5);
    // the regeneration was asked from the question's history, without repeating the question
    let sent = streamed_requests(&rig.server, false).await;
    let regenerated = sent[2]["messages"].to_string();
    assert_eq!(
        regenerated.matches("second question").count(),
        1,
        "{regenerated}"
    );
}

#[allow(clippy::await_holding_lock)]
#[quench_test::test]
async fn regenerate_refuses_what_it_cannot_redo() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let rig = rig().await;
    let repo = rig.db.repository::<Message>();
    let message = |id: &str, role: &str, parent: Option<&str>| Message {
        id: id.to_string(),
        conversation_id: "conv".to_string(),
        parent_id: parent.map(str::to_string),
        role: role.to_string(),
        content: "text".to_string(),
        created_at: "2026-01-01T00:00:00Z".to_string(),
    };
    repo.create(&message("user-msg", "user", None))
        .await
        .unwrap();
    repo.create(&message("orphan", "assistant", None))
        .await
        .unwrap();
    repo.create(&message("lost", "assistant", Some("gone")))
        .await
        .unwrap();
    repo.create(&message("good", "assistant", Some("user-msg")))
        .await
        .unwrap();

    let status_for = |id: &'static str| {
        let rig = &rig;
        async move {
            rig.app
                .call(form(
                    "/ui/chat/regenerate",
                    &[("message_id", id)],
                    &rig.container,
                ))
                .await
                .status()
        }
    };
    assert_eq!(status_for("unknown").await, StatusCode::NOT_FOUND);
    assert_eq!(status_for("user-msg").await, StatusCode::BAD_REQUEST);
    assert_eq!(status_for("orphan").await, StatusCode::BAD_REQUEST);
    assert_eq!(status_for("lost").await, StatusCode::NOT_FOUND);
    assert_eq!(status_for("good").await, StatusCode::OK);

    // with only an embedding model running there is nothing to regenerate with
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/token"))
        .respond_with(crate::mock_estate::json(
            200,
            &quench_serde::json!({"access_token": "t", "expires_in": 3600}),
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/vllm/instances"))
        .respond_with(crate::mock_estate::json(
            200,
            &quench_serde::json!([crate::mock_estate::instance_json(
                EMBEDDING_MODEL,
                server.address().port(),
                Some("embed")
            )]),
        ))
        .mount(&server)
        .await;
    quench_core::env::set("SWITCHBOARD_URL", server.uri());
    quench_core::env::set("GATEHOUSE_URL", server.uri());
    let embed_only = rig_with(server, SwitchboardClient::new(), "", JwtConfig::for_tests()).await;
    embed_only
        .db
        .repository::<Message>()
        .create(&message("user-msg", "user", None))
        .await
        .unwrap();
    embed_only
        .db
        .repository::<Message>()
        .create(&message("good", "assistant", Some("user-msg")))
        .await
        .unwrap();
    let resp = embed_only
        .app
        .call(form(
            "/ui/chat/regenerate",
            &[("message_id", "good")],
            &embed_only.container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[allow(clippy::await_holding_lock)]
#[quench_test::test]
async fn a_refusing_vllm_shows_an_error_and_keeps_the_message_pending() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let rig = rig().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(503).set_body_raw("overloaded".to_string(), "text/plain"),
        )
        .mount(&rig.server)
        .await;

    let (status, body) = stream(&rig, "m1", request(CHAT_ID, "hi", "c1")).await;
    assert_eq!(status, StatusCode::OK, "the error is part of the stream");
    assert!(body.contains("ui_chat_error"), "{body}");
    assert!(body.contains("overloaded"), "{body}");
    assert!(rig.state.pending_messages.get("m1").is_some());
    // nothing but the pre-created conversation was stored
    assert!(messages(&rig.db).await.is_empty());
}

#[allow(clippy::await_holding_lock)]
#[quench_test::test]
async fn a_tool_call_in_the_answer_is_run_summarised_and_embedded() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (server, switchboard) = mock_estate(&[]).await;
    let rig = rig_with(
        server,
        switchboard,
        "Search results for 'rust'\n\nRust is a systems language.\nSource: https://rust-lang.org/",
        JwtConfig::for_tests(),
    )
    .await;
    // The summary request is told apart by its prompt, and must be tried before the general rule.
    on_chat(
        &rig.server,
        "Provide the information clearly",
        sse(&["Rust is a ", "systems language."]),
    )
    .await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(sse(&[
            "Let me look.\n",
            "<tool_call>{\"name\": \"web_search\", \"arguments\": {\"query\": \"rust\"}}</tool_call>",
        ]))
        .mount(&rig.server)
        .await;

    let (_, body) = stream(&rig, "m1", request(CHAT_ID, "look up rust", "c1")).await;
    assert!(body.contains("systems language."), "{body}");
    // the raw call shows while it is typed out, and the final message replaces it
    let last = body.rsplit("event: message").next().unwrap();
    let message = last
        .split("hx-swap-oob=\"outerHTML:#dot-ai-m1\"")
        .next()
        .unwrap();
    assert!(!message.contains("&lt;tool_call"), "{message}");
    assert!(message.contains("tool-result"), "{message}");

    let stored = messages(&rig.db).await;
    let answer = stored
        .iter()
        .find(|m| m.role == "assistant")
        .expect("an answer");
    assert!(
        answer.content.contains("Rust is a systems language."),
        "{}",
        answer.content
    );
    assert_eq!(streamed_requests(&rig.server, true).await.len(), 1);
}

#[allow(clippy::await_holding_lock)]
#[quench_test::test]
async fn a_question_about_the_tools_does_not_run_them() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let rig = rig().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(sse(&[
            "I can search: <tool_call>{\"name\": \"web_search\", \"arguments\": {\"query\": \"x\"}}</tool_call>",
        ]))
        .mount(&rig.server)
        .await;
    stream(
        &rig,
        "m1",
        request(CHAT_ID, "What tools do you have?", "c1"),
    )
    .await;
    assert!(streamed_requests(&rig.server, true).await.is_empty());
    assert_eq!(messages(&rig.db).await.len(), 2);
}

#[allow(clippy::await_holding_lock)]
#[quench_test::test]
async fn a_requested_profile_replaces_the_default_and_an_unknown_one_is_ignored() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let rig = rig().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(sse(&["ok"]))
        .mount(&rig.server)
        .await;
    let mut req = request(CHAT_ID, "hi", "c1");
    req.capability_profile = Some("no-such-profile".to_string());
    req.tool_confirmations = vec!["command".to_string()];
    let (status, _) = stream(&rig, "m1", req).await;
    assert_eq!(status, StatusCode::OK);

    let mut req = request(CHAT_ID, "hi again", "c2");
    req.capability_profile = Some("web_assistant".to_string());
    req.project_id = Some("proj".to_string());
    let (status, _) = stream(&rig, "m2", req).await;
    assert_eq!(status, StatusCode::OK);
    let conv = rig
        .db
        .repository::<Conversation>()
        .read("c2")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(conv.project_id.as_deref(), Some("proj"));
}

#[allow(clippy::await_holding_lock)]
#[quench_test::test]
async fn a_grounded_turn_shows_progress_cites_sources_and_fact_checks() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _grounded = GroundedEnv::on();
    let (server, switchboard) = mock_estate(&[]).await;
    let rig = rig_with(
        server,
        switchboard,
        "Search results for 'tower'\n\nThe tower is 330 metres high.\nSource: https://tower.example/facts",
        JwtConfig::for_tests(),
    )
    .await;
    on_chat(
        &rig.server,
        "retrieval planner",
        completion(r#"{"kind":"factual","queries":["tower height"]}"#),
    )
    .await;
    on_chat(
        &rig.server,
        "careful fact-checker",
        completion(
            r#"{"notice":"Not in the sources:","unsupported":[{"claim":"It is pink","severity":"core","reason":"no source"}]}"#,
        ),
    )
    .await;
    Mock::given(method("POST"))
        .and(path("/v1/embeddings"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&rig.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(body_string_contains("\"stream\":true"))
        .respond_with(sse(&["The tower is 330 metres high [1]. It is pink [1]."]))
        .mount(&rig.server)
        .await;

    let (status, body) = stream(&rig, "m1", request(CHAT_ID, "How high is the tower?", "c1")).await;
    assert_eq!(status, StatusCode::OK);
    // progress while the harness works, then the checking line, then the answer with its warning
    assert!(body.contains("ui_chat_status_planning"), "{body}");
    assert!(body.contains("ui_chat_status_searching"), "{body}");
    assert!(body.contains("tower height"), "{body}");
    assert!(body.contains("ui_chat_status_checking"), "{body}");
    assert!(body.contains("Not in the sources:"), "{body}");
    assert!(
        body.contains("tower.example"),
        "the cited source is listed: {body}"
    );

    // grounded mode retrieves up front, so no native tools are offered
    let sent = streamed_requests(&rig.server, false).await;
    assert!(
        sent[0]
            .get("tools")
            .is_none_or(quench_serde::Value::is_null)
    );
    assert!(sent[0]["messages"].to_string().contains("WEB SOURCES"));
}

#[allow(clippy::await_holding_lock)]
#[quench_test::test]
async fn a_regeneration_of_a_grounded_answer_researches_the_previous_question() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _grounded = GroundedEnv::on();
    let (server, switchboard) = mock_estate(&[]).await;
    let rig = rig_with(
        server,
        switchboard,
        "Search results for 'x'\n\nA fact.\nSource: https://facts.example/a",
        JwtConfig::for_tests(),
    )
    .await;
    on_chat(
        &rig.server,
        "retrieval planner",
        completion(r#"{"kind":"factual","queries":["a fact"]}"#),
    )
    .await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(body_string_contains("\"stream\":true"))
        .respond_with(sse(&["A fact [1]."]))
        .mount(&rig.server)
        .await;
    // fact-checking is not mocked: the refused call must not alter the answer
    Mock::given(method("POST"))
        .and(path("/v1/embeddings"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&rig.server)
        .await;

    let repo = rig.db.repository::<Message>();
    for (id, role, parent, content, at) in [
        ("q", "user", None, "tell me a fact", "2026-01-01T00:00:00Z"),
        (
            "a",
            "assistant",
            Some("q"),
            "an earlier answer",
            "2026-01-01T00:00:01Z",
        ),
    ] {
        repo.create(&Message {
            id: id.to_string(),
            conversation_id: "c1".to_string(),
            parent_id: parent.map(str::to_string),
            role: role.to_string(),
            content: content.to_string(),
            created_at: at.to_string(),
        })
        .await
        .unwrap();
    }
    let mut req = request(CHAT_ID, "", "c1");
    req.skip_user_message = true;
    req.parent_id = Some("q".to_string());
    let (status, body) = stream(&rig, "m1", req).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("ui_chat_status_searching"), "{body}");
    assert!(body.contains("A fact [1]."), "{body}");
    let planner = rig
        .server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .find(|r| r.body_string().contains("retrieval planner"))
        .map(quench_test::mock::Request::body_string)
        .expect("a planner call");
    assert!(planner.contains("tell me a fact"), "{planner}");
}

#[allow(clippy::await_holding_lock)]
#[quench_test::test]
async fn the_refreshed_sidebar_lists_the_users_projects_with_the_active_one_open() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let rig = rig().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(sse(&["Noted."]))
        .mount(&rig.server)
        .await;
    let projects = rig.db.repository::<sage_service::domain::models::Project>();
    for (id, name, owner, at) in [
        ("p1", "Atlas", "admin", "2026-01-02T00:00:00Z"),
        ("p2", "Borealis", "admin", "2026-01-01T00:00:00Z"),
        ("p3", "Someone else's", "mallory", "2026-01-03T00:00:00Z"),
    ] {
        projects
            .create(&sage_service::domain::models::Project {
                id: id.to_string(),
                name: name.to_string(),
                owner: owner.to_string(),
                created_at: at.to_string(),
                updated_at: at.to_string(),
            })
            .await
            .unwrap();
    }
    // a conversation of the active project, and one outside any project
    let conversations = rig.db.repository::<Conversation>();
    for (id, title, project) in [
        ("in-p1", "Inside Atlas", Some("p1")),
        ("loose", "Loose chat", None),
    ] {
        conversations
            .create(&Conversation {
                id: id.to_string(),
                title: title.to_string(),
                active_message_id: None,
                owner: "admin".to_string(),
                project_id: project.map(str::to_string),
                updated_at: "2026-01-01T00:00:00Z".to_string(),
            })
            .await
            .unwrap();
    }

    let mut req = request(CHAT_ID, "plan the atlas", "in-p1");
    req.project_id = Some("p1".to_string());
    let (status, body) = stream(&rig, "m1", req).await;
    assert_eq!(status, StatusCode::OK);

    assert!(
        body.contains("fa-folder-open"),
        "the active project is open: {body}"
    );
    assert!(
        body.contains("Atlas") && body.contains("Borealis"),
        "{body}"
    );
    assert!(
        !body.contains("Someone else"),
        "other users' projects stay hidden: {body}"
    );
    assert!(body.contains("project-conv-item"), "{body}");
    assert!(
        body.contains("conversation_id=in-p1&amp;project_id=p1"),
        "{body}"
    );
    assert!(body.contains("Loose chat"), "{body}");
    // the project link is kept even though this message carried no title of its own
    let conv = conversations.read("in-p1").await.unwrap().unwrap();
    assert_eq!(conv.project_id.as_deref(), Some("p1"));
}
