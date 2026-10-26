//! The network-facing half of `grounding`: `gather` (plan, search, fetch, rank) and
//! `finalize_answer` (citation repair, fact-check), run against one mock server that stands in
//! for gatehouse, switchboard, vLLM and the web page being read.

use crate::env_support::env_lock;
use quench_test::mock::matchers::{body_string_contains, method, path};
use quench_test::mock::{Mock, MockServer, ResponseTemplate};
use sage_service::clients::switchboard::{SwitchboardClient, VllmInstance};
use sage_service::clients::vllm::{ChatMessage, VllmClient};
use sage_service::grounding::*;
use sage_service::tools::{SearchProvider, SearchProviderRegistry};
use std::time::Duration;

const EMBEDDING_MODEL: &str = "Qwen/Qwen3-Embedding-0.6B";

const fn config() -> GroundingConfig {
    GroundingConfig {
        enabled: true,
        max_queries: 3,
        fetch_pages: 0,
        passages: 6,
        max_chars: 6000,
        timeout: Duration::from_secs(10),
        verify: false,
        verifier_model: None,
        verify_timeout: Duration::from_secs(10),
        cite_repair: false,
    }
}

fn json(status: u16, body: &quench_serde::Value) -> ResponseTemplate {
    ResponseTemplate::new(status)
        .set_body_raw(quench_serde::json::to_string(body), "application/json")
}

/// A chat completion whose message content is `content`.
fn completion(content: &str) -> ResponseTemplate {
    json(
        200,
        &quench_serde::json!({"choices": [{"message": {"content": content}}]}),
    )
}

fn instance_json(model: &str, port: u16, task: Option<&str>) -> quench_serde::Value {
    quench_serde::json!({
        "id": format!("id-{model}"),
        "namespace": "ns",
        "model": model,
        "host": "127.0.0.1",
        "port": port,
        "quantization": null,
        "max_model_len": null,
        "gpu_memory_utilization": null,
        "enable_prefix_caching": false,
        "task": task,
        "started_at": quench_core::time::Time::now().to_rfc3339(),
        "status": "running"
    })
}

fn answerer(server: &MockServer) -> VllmInstance {
    quench_serde::json::from_str(&quench_serde::json::to_string(&instance_json(
        "chat-model",
        server.address().port(),
        None,
    )))
    .expect("an instance")
}

/// Mounts the token endpoint and a switchboard that lists the embedding model and `extra` models.
async fn mock_estate(extra: &[&str]) -> (MockServer, SwitchboardClient) {
    let server = MockServer::start().await;
    let port = server.address().port();
    Mock::given(method("POST"))
        .and(path("/api/v1/token"))
        .respond_with(json(
            200,
            &quench_serde::json!({"access_token": "t", "expires_in": 3600}),
        ))
        .mount(&server)
        .await;
    let mut instances = vec![instance_json(EMBEDDING_MODEL, port, Some("embed"))];
    instances.extend(extra.iter().map(|m| instance_json(m, port, None)));
    Mock::given(method("GET"))
        .and(path("/api/v1/vllm/instances"))
        .respond_with(json(200, &quench_serde::Value::Array(instances)))
        .mount(&server)
        .await;
    quench_core::env::set("SWITCHBOARD_URL", server.uri());
    quench_core::env::set("GATEHOUSE_URL", server.uri());
    quench_core::env::set("CLIENT_SECRET_SAGE_SWITCHBOARD", "secret");
    quench_core::env::set("SAGE_EMBEDDING_MODEL", EMBEDDING_MODEL);
    let switchboard = SwitchboardClient::new();
    (server, switchboard)
}

async fn on_chat(server: &MockServer, prompt_marker: &str, response: ResponseTemplate) {
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(body_string_contains(prompt_marker))
        .respond_with(response)
        .mount(server)
        .await;
}

struct Search(String);

#[quench_core::async_trait]
impl SearchProvider for Search {
    fn name(&self) -> &'static str {
        "fake"
    }
    fn requires_api_key(&self) -> bool {
        false
    }
    async fn search(&self, _query: &str) -> Result<String, String> {
        Ok(self.0.clone())
    }
}

fn registry(results: &str) -> SearchProviderRegistry {
    let mut registry = SearchProviderRegistry::new();
    registry.register("searxng".to_string(), Box::new(Search(results.to_string())));
    registry
}

fn results(hits: &[(&str, &str)]) -> String {
    let entries: Vec<String> = hits
        .iter()
        .map(|(snippet, url)| format!("{snippet}\nSource: {url}"))
        .collect();
    format!("Search results for 'q'\n\n{}", entries.join("\n---\n"))
}

fn history() -> Vec<ChatMessage> {
    ["user", "assistant", "system"]
        .iter()
        .map(|role| ChatMessage {
            role: (*role).to_string(),
            content: format!("{role} said something"),
            tool_calls: None,
            images: None,
        })
        .collect()
}

#[allow(clippy::await_holding_lock)] // env vars are process-global; held for the whole test
#[quench_test::test]
async fn gather_returns_nothing_when_the_planner_says_no_search_is_needed() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (server, switchboard) = mock_estate(&[]).await;
    on_chat(
        &server,
        "retrieval planner",
        completion(r#"{"kind":"chitchat","queries":[]}"#),
    )
    .await;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let grounding = gather(
        &config(),
        &switchboard,
        &VllmClient::new(),
        &registry(""),
        "searxng",
        &answerer(&server),
        &history(),
        "thanks!",
        "2026-10-26",
        Some(&tx),
    )
    .await;
    assert!(grounding.is_none());
    assert_eq!(rx.try_recv().expect("a stage"), Stage::Planning);
    assert!(rx.try_recv().is_err(), "no search stage follows");

    // The planner saw the recent conversation but not the system turn.
    let sent = server.received_requests().await.expect("recorded");
    let planner = sent
        .iter()
        .find(|r| r.body_string().contains("retrieval planner"))
        .expect("a planner call");
    let body = planner.body_string();
    assert!(body.contains("user said something"), "{body}");
    assert!(body.contains("assistant said something"), "{body}");
    assert!(!body.contains("system said something"), "{body}");
}

#[allow(clippy::await_holding_lock)]
#[quench_test::test]
async fn gather_ranks_the_snippets_by_embedding_similarity() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (server, switchboard) = mock_estate(&[]).await;
    on_chat(
        &server,
        "retrieval planner",
        completion(r#"{"kind":"factual","queries":["height of the tower"]}"#),
    )
    .await;
    // vectors: the query, then the two snippets - the second one is the closer match
    Mock::given(method("POST"))
        .and(path("/v1/embeddings"))
        .respond_with(json(
            200,
            &quench_serde::json!({"data": [
                {"index": 0, "embedding": [1.0, 0.0]},
                {"index": 1, "embedding": [0.0, 1.0]},
                {"index": 2, "embedding": [1.0, 0.1]},
            ]}),
        ))
        .mount(&server)
        .await;

    let mut cfg = config();
    cfg.passages = 1;
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let grounding = gather(
        &cfg,
        &switchboard,
        &VllmClient::new(),
        &registry(&results(&[
            ("It is tall in the south.", "https://far.example/a"),
            ("The tower is 330 metres high.", "https://near.example/b"),
        ])),
        "searxng",
        &answerer(&server),
        &[],
        "How high is the tower?",
        "2026-10-26",
        Some(&tx),
    )
    .await
    .expect("grounded");

    assert!(!grounding.unavailable);
    assert!(
        grounding.block.contains("330 metres"),
        "{}",
        grounding.block
    );
    assert!(!grounding.block.contains("tall in the south"));
    assert_eq!(grounding.sources.len(), 1);
    assert_eq!(grounding.sources[0].domain, "near.example");
    assert!(grounding.evidence.contains("330 metres"));

    assert_eq!(rx.try_recv().unwrap(), Stage::Planning);
    assert_eq!(
        rx.try_recv().unwrap(),
        Stage::Searching(vec!["height of the tower".to_string()])
    );
    assert_eq!(rx.try_recv().unwrap(), Stage::Reading(0));
}

#[allow(clippy::await_holding_lock)]
#[quench_test::test]
async fn gather_reads_pages_and_falls_back_to_natural_order_without_an_embedder() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (server, switchboard) = mock_estate(&[]).await;
    on_chat(
        &server,
        "retrieval planner",
        // a duplicate query is dropped; the planner is told the year via the system prompt
        completion(r#"{"kind":"current","queries":["tower height","Tower Height"]}"#),
    )
    .await;
    Mock::given(method("POST"))
        .and(path("/v1/embeddings"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/page"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            "<html><head><title>Tower guide</title></head><body><article><p>The observation deck sits at 276 metres above the street level.</p><p>Visitors reach it by a lift that takes under a minute to arrive.</p></article></body></html>".to_string(),
            "text/html",
        ))
        .mount(&server)
        .await;
    // a page that cannot be read still leaves its snippet
    Mock::given(method("GET"))
        .and(path("/missing"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    let mut cfg = config();
    cfg.fetch_pages = 2;
    let page = format!("{}/page", server.uri());
    let missing = format!("{}/missing", server.uri());
    let grounding = gather(
        &cfg,
        &switchboard,
        &VllmClient::new(),
        &registry(&results(&[
            ("Snippet about the deck.", &page),
            ("Snippet from the broken page.", &missing),
        ])),
        "searxng",
        &answerer(&server),
        &[],
        "How high is the deck?",
        "2026-10-26",
        None,
    )
    .await
    .expect("grounded");

    assert!(!grounding.unavailable);
    assert!(
        grounding.block.contains("276 metres"),
        "{}",
        grounding.block
    );
    assert!(grounding.block.contains("Snippet from the broken page."));
    assert_eq!(grounding.sources.len(), 2);

    let sent = server.received_requests().await.expect("recorded");
    let planner = sent
        .iter()
        .find(|r| r.body_string().contains("retrieval planner"))
        .expect("a planner call");
    assert!(planner.body_string().contains("2026-10-26"));
}

#[allow(clippy::await_holding_lock)]
#[quench_test::test]
async fn gather_searches_the_raw_question_when_the_planner_fails() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (server, switchboard) = mock_estate(&[]).await;
    on_chat(&server, "retrieval planner", ResponseTemplate::new(500)).await;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let grounding = gather(
        &config(),
        &switchboard,
        &VllmClient::new(),
        &registry(&results(&[("One lone snippet.", "https://only.example/x")])),
        "searxng",
        &answerer(&server),
        &[],
        "What is the capital of Peru?",
        "2026-10-26",
        Some(&tx),
    )
    .await
    .expect("grounded");
    assert!(!grounding.unavailable);
    assert!(grounding.block.contains("One lone snippet."));
    assert_eq!(rx.try_recv().unwrap(), Stage::Planning);
    assert_eq!(
        rx.try_recv().unwrap(),
        Stage::Searching(vec!["What is the capital of Peru?".to_string()])
    );
}

#[allow(clippy::await_holding_lock)]
#[quench_test::test]
async fn gather_says_so_when_the_search_finds_nothing_usable() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (server, switchboard) = mock_estate(&[]).await;
    on_chat(
        &server,
        "retrieval planner",
        completion(r#"{"kind":"factual","queries":["anything"]}"#),
    )
    .await;

    let grounding = gather(
        &config(),
        &switchboard,
        &VllmClient::new(),
        &registry("Search results for 'q'\n\nnothing usable here"),
        "searxng",
        &answerer(&server),
        &[],
        "Anything?",
        "2026-10-26",
        None,
    )
    .await
    .expect("a notice, not silence");
    assert!(grounding.unavailable);
    assert!(grounding.sources.is_empty());
    assert!(
        grounding.block.contains("no usable results"),
        "{}",
        grounding.block
    );
}

#[allow(clippy::await_holding_lock)]
#[quench_test::test]
async fn gather_gives_up_with_a_notice_when_the_planner_hangs() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (server, switchboard) = mock_estate(&[]).await;
    // A vLLM that accepts the connection and never answers.
    let hang = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let hang_port = hang.local_addr().unwrap().port();
    let _held = tokio::spawn(async move {
        while let Ok((stream, _)) = hang.accept().await {
            // keep the connection open and never answer
            tokio::spawn(async move {
                let _stream = stream;
                std::future::pending::<()>().await;
            });
        }
    });
    let mut stuck = answerer(&server);
    stuck.port = hang_port;

    let mut cfg = config();
    cfg.timeout = Duration::from_millis(200);
    let grounding = gather(
        &cfg,
        &switchboard,
        &VllmClient::new(),
        &registry(""),
        "searxng",
        &stuck,
        &[],
        "Hello?",
        "2026-10-26",
        None,
    )
    .await
    .expect("a notice");
    assert!(grounding.unavailable);
    assert!(grounding.block.contains("timed out"), "{}", grounding.block);
}

const EVIDENCE: &str = "[1] example.com\nThe tower is 330 metres high.";

#[allow(clippy::await_holding_lock)]
#[quench_test::test]
async fn finalize_answer_without_evidence_flags_the_answer_as_unsourced() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (server, switchboard) = mock_estate(&[]).await;
    let out = finalize_answer(
        &config(),
        &switchboard,
        &VllmClient::new(),
        &answerer(&server),
        "See https://invented.example for details.",
        0,
        "",
        "2026-10-26",
        "Tell me about it",
    )
    .await;
    assert!(!out.contains("invented.example"), "{out}");
    assert!(out.contains("No sources were found"), "{out}");
}

#[allow(clippy::await_holding_lock)]
#[quench_test::test]
async fn finalize_answer_asks_for_citations_once_and_keeps_a_usable_rewrite() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (server, switchboard) = mock_estate(&[]).await;
    on_chat(
        &server,
        "You edit a draft answer",
        completion(r#"{"answer":"The tower is 330 metres high [1]."}"#),
    )
    .await;
    let mut cfg = config();
    cfg.cite_repair = true;
    let out = finalize_answer(
        &cfg,
        &switchboard,
        &VllmClient::new(),
        &answerer(&server),
        "The tower is 330 metres high.",
        1,
        EVIDENCE,
        "2026-10-26",
        "How high is the tower?",
    )
    .await;
    assert_eq!(out, "The tower is 330 metres high [1].");
}

#[allow(clippy::await_holding_lock)]
#[quench_test::test]
async fn finalize_answer_keeps_the_original_when_the_repair_is_unusable_or_fails() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut cfg = config();
    cfg.cite_repair = true;
    let original = "The tower is 330 metres high.";

    for response in [
        // cites a source that does not exist
        completion(r#"{"answer":"It is tall [9]."}"#),
        // valid completion, wrong shape
        completion(r#"{"unexpected":true}"#),
        completion("not json at all"),
        ResponseTemplate::new(500),
    ] {
        let (server, switchboard) = mock_estate(&[]).await;
        on_chat(&server, "You edit a draft answer", response).await;
        let out = finalize_answer(
            &cfg,
            &switchboard,
            &VllmClient::new(),
            &answerer(&server),
            original,
            1,
            EVIDENCE,
            "2026-10-26",
            "How high is the tower?",
        )
        .await;
        assert_eq!(out, original);
    }
}

#[allow(clippy::await_holding_lock)]
#[quench_test::test]
async fn finalize_answer_appends_the_fact_check_warning() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (server, switchboard) = mock_estate(&[]).await;
    on_chat(
        &server,
        "careful fact-checker",
        completion(
            r#"{"notice":"Not in the sources:","unsupported":[{"claim":"It opened in 1950","severity":"core","reason":"no source says so"},{"claim":"minor","severity":"detail","reason":""}]}"#,
        ),
    )
    .await;
    let mut cfg = config();
    cfg.verify = true;
    let out = finalize_answer(
        &cfg,
        &switchboard,
        &VllmClient::new(),
        &answerer(&server),
        "The tower is 330 metres high [1]. It opened in 1950 [1].",
        1,
        EVIDENCE,
        "2026-10-26",
        "How high is the tower?",
    )
    .await;
    assert!(
        out.starts_with("The tower is 330 metres high [1]."),
        "{out}"
    );
    assert!(out.contains("⚠ Not in the sources:"), "{out}");
    assert!(
        out.contains("- It opened in 1950 — no source says so"),
        "{out}"
    );
    assert!(!out.contains("minor"), "{out}");
}

#[allow(clippy::await_holding_lock)]
#[quench_test::test]
async fn verify_uses_the_configured_model_when_running_else_the_answerer_and_fails_open() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (server, switchboard) = mock_estate(&["judge-model"]).await;
    on_chat(
        &server,
        "careful fact-checker",
        completion(
            r#"{"notice":"","unsupported":[{"claim":"x","severity":"contradicted","reason":"r"}]}"#,
        ),
    )
    .await;
    let vllm = VllmClient::new();
    let mut cfg = config();
    cfg.verify = true;

    // a verifier that is running is the one asked
    cfg.verifier_model = Some("judge-model".to_string());
    let note = verify(
        &cfg,
        &switchboard,
        &vllm,
        &answerer(&server),
        "A.",
        EVIDENCE,
        "d",
    )
    .await
    .expect("a warning");
    assert!(note.contains("could not be verified"), "{note}");
    let sent = server.received_requests().await.expect("recorded");
    let asked = sent
        .iter()
        .find(|r| r.body_string().contains("careful fact-checker"))
        .expect("a verifier call");
    assert_eq!(asked.body_json().expect("json")["model"], "judge-model");

    // one that is not running falls back to the answerer
    cfg.verifier_model = Some("absent-model".to_string());
    assert!(
        verify(
            &cfg,
            &switchboard,
            &vllm,
            &answerer(&server),
            "A.",
            EVIDENCE,
            "d"
        )
        .await
        .is_some()
    );
    let sent = server.received_requests().await.expect("recorded");
    let last = sent
        .iter()
        .rev()
        .find(|r| r.body_string().contains("careful fact-checker"))
        .expect("a verifier call");
    assert_eq!(last.body_json().expect("json")["model"], "chat-model");

    // a verifier that errors never blocks the answer
    let (broken, switchboard) = mock_estate(&[]).await;
    on_chat(&broken, "careful fact-checker", ResponseTemplate::new(500)).await;
    cfg.verifier_model = None;
    assert!(
        verify(
            &cfg,
            &switchboard,
            &VllmClient::new(),
            &answerer(&broken),
            "A.",
            EVIDENCE,
            "d"
        )
        .await
        .is_none()
    );
}
