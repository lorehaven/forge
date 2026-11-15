//! The network-facing half of `grounding`: `gather` (plan, search, fetch, rank) and
//! `finalize_answer` (citation repair, fact-check), run against one mock server that stands in
//! for gatehouse, switchboard, vLLM and the web page being read.

use crate::env_support::env_lock;
use crate::mock_estate::{answerer, completion, json, mock_estate, on_chat};
use quench_test::mock::matchers::{method, path};
use quench_test::mock::{Mock, ResponseTemplate};
use sage_service::clients::vllm::{ChatMessage, VllmClient};
use sage_service::grounding::*;
use sage_service::tools::{SearchProvider, SearchProviderRegistry};
use std::time::Duration;

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
        lang_repair: false,
        wikipedia: false,
        min_score: 0.0,
        rerank_model: String::new(),
        rerank_top: 24,
    }
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
    // the page server of this test is on loopback, which the fetcher refuses by default
    quench_core::env::set("SAGE_FETCH_ALLOW_PRIVATE", "true");
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
    quench_core::env::remove("SAGE_FETCH_ALLOW_PRIVATE");
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
    assert_eq!(
        grounding.sources,
        [] as [sage_service::grounding::WebSource; 0]
    );
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

#[allow(clippy::await_holding_lock)]
#[quench_test::test]
async fn finalize_answer_never_returns_a_blank_answer() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (server, switchboard) = mock_estate(&[]).await;
    for (answer, question, expected) in [
        ("", "Jaki jest numer telefonu restauracji?", "Nie znalazłem"),
        (
            "<tool_call>{\"name\":\"web_search\"}</tool_call>",
            "What is the phone number?",
            "I could not find a verified answer",
        ),
    ] {
        // with sources
        let out = finalize_answer(
            &config(),
            &switchboard,
            &VllmClient::new(),
            &answerer(&server),
            answer,
            1,
            "[1] a.example\nsome text",
            "2026-11-14",
            question,
        )
        .await;
        assert!(out.contains(expected), "{out}");
        // without sources: still says so
        let out = finalize_answer(
            &config(),
            &switchboard,
            &VllmClient::new(),
            &answerer(&server),
            answer,
            0,
            "",
            "2026-11-14",
            question,
        )
        .await;
        assert!(out.contains(expected) && out.contains('ℹ'), "{out}");
    }
}

#[allow(clippy::await_holding_lock)]
#[quench_test::test]
async fn the_page_fetcher_refuses_private_addresses_by_default() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    quench_core::env::remove("SAGE_FETCH_ALLOW_PRIVATE");
    let client = sage_service::tools::web_fetch::fetch_client();
    for url in [
        "http://127.0.0.1:9/x",
        "http://169.254.169.254/latest/meta-data",
        "http://localhost/",
    ] {
        let err = sage_service::tools::web_fetch::fetch_page_text(&client, url, 1000)
            .await
            .unwrap_err();
        assert!(err.contains("Blocked"), "{url}: {err}");
    }
}

/// Runs `gather` against a mocked planner and embedder: the query is `[1, 0]`, the snippets get `snippet_vectors`.
async fn gather_with_vectors(
    floor: f32,
    snippet_vectors: &[[f32; 2]],
    hits: &[(&str, &str)],
) -> Grounding {
    let (server, switchboard) = mock_estate(&[]).await;
    on_chat(
        &server,
        "retrieval planner",
        completion(r#"{"kind":"factual","queries":["height of the tower"]}"#),
    )
    .await;
    let mut data = vec![quench_serde::json!({"index": 0, "embedding": [1.0, 0.0]})];
    for (i, v) in snippet_vectors.iter().enumerate() {
        data.push(quench_serde::json!({"index": i + 1, "embedding": [v[0], v[1]]}));
    }
    Mock::given(method("POST"))
        .and(path("/v1/embeddings"))
        .respond_with(json(200, &quench_serde::json!({ "data": data })))
        .mount(&server)
        .await;
    let mut cfg = config();
    cfg.min_score = floor;
    gather(
        &cfg,
        &switchboard,
        &VllmClient::new(),
        &registry(&results(hits)),
        "searxng",
        &answerer(&server),
        &[],
        "How high is the tower?",
        "2026-11-14",
        None,
    )
    .await
    .expect("a grounding, even an empty one")
}

#[allow(clippy::await_holding_lock)]
#[quench_test::test]
async fn gather_drops_passages_below_the_relevance_floor() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    // the first snippet is orthogonal to the query (similarity 0), the second nearly parallel
    let g = gather_with_vectors(
        0.3,
        &[[0.0, 1.0], [1.0, 0.1]],
        &[
            ("Cookies are used on this site.", "https://noise.example/a"),
            ("The tower is 330 metres high.", "https://near.example/b"),
        ],
    )
    .await;
    assert!(!g.unavailable);
    assert!(g.block.contains("330 metres"));
    assert!(!g.block.contains("Cookies"), "{}", g.block);
    assert_eq!(g.sources.len(), 1);
}

#[allow(clippy::await_holding_lock)]
#[quench_test::test]
async fn gather_with_nothing_relevant_counts_as_no_sources() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let g = gather_with_vectors(
        0.3,
        &[[0.0, 1.0], [0.1, 1.0]],
        &[
            ("Cookies are used on this site.", "https://noise.example/a"),
            ("Careers at Red Hat.", "https://noise.example/b"),
        ],
    )
    .await;
    assert!(g.unavailable);
    assert!(g.sources.is_empty() && g.evidence.is_empty());
    assert!(g.block.contains("not relevant"), "{}", g.block);
    // with the floor off the same results are used
    let g = gather_with_vectors(
        0.0,
        &[[0.0, 1.0], [0.1, 1.0]],
        &[
            ("Cookies are used on this site.", "https://noise.example/a"),
            ("Careers at Red Hat.", "https://noise.example/b"),
        ],
    )
    .await;
    assert!(!g.unavailable);
}

#[allow(clippy::await_holding_lock)]
#[quench_test::test]
async fn finalize_answer_rewrites_an_answer_that_came_out_in_the_wrong_language() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (server, switchboard) = mock_estate(&[]).await;
    on_chat(
        &server,
        "You rewrite an answer",
        completion(
            r#"{"answer":"The tower is 330 metres high according to the official guide [1]."}"#,
        ),
    )
    .await;
    let mut cfg = config();
    cfg.lang_repair = true;
    let polish = "Wieża ma 330 metrów wysokości według oficjalnego przewodnika, który jest dostępny dla wszystkich [1].";
    let out = finalize_answer(
        &cfg,
        &switchboard,
        &VllmClient::new(),
        &answerer(&server),
        polish,
        1,
        EVIDENCE,
        "2026-11-14",
        "How high is the tower according to the guide?",
    )
    .await;
    assert_eq!(
        out,
        "The tower is 330 metres high according to the official guide [1]."
    );

    // the question is in Polish: a Polish answer is left alone (no call needed)
    let out = finalize_answer(
        &cfg,
        &switchboard,
        &VllmClient::new(),
        &answerer(&server),
        polish,
        1,
        EVIDENCE,
        "2026-11-14",
        "Jak wysoka jest wieża według oficjalnego przewodnika?",
    )
    .await;
    assert_eq!(out, polish);
}

#[allow(clippy::await_holding_lock)]
#[quench_test::test]
async fn finalize_answer_keeps_the_original_when_the_language_rewrite_is_no_better() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (server, switchboard) = mock_estate(&[]).await;
    // still Polish, and the citation is gone
    on_chat(
        &server,
        "You rewrite an answer",
        completion(r#"{"answer":"Wieża ma 330 metrów wysokości według przewodnika, który jest dostępny dla wszystkich."}"#),
    )
    .await;
    let mut cfg = config();
    cfg.lang_repair = true;
    let polish = "Wieża ma 330 metrów wysokości według oficjalnego przewodnika, który jest dostępny dla wszystkich [1].";
    let out = finalize_answer(
        &cfg,
        &switchboard,
        &VllmClient::new(),
        &answerer(&server),
        polish,
        1,
        EVIDENCE,
        "2026-11-14",
        "How high is the tower according to the guide?",
    )
    .await;
    assert_eq!(out, polish);
}

#[allow(clippy::await_holding_lock)]
#[quench_test::test]
async fn finalize_answer_without_sources_refuses_instead_of_answering_from_memory() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (server, switchboard) = mock_estate(&[]).await;
    let ask = |answer: &'static str, question: &'static str| {
        let (server, switchboard) = (&server, &switchboard);
        async move {
            finalize_answer(
                &config(),
                switchboard,
                &VllmClient::new(),
                &answerer(server),
                answer,
                0,
                "",
                "2026-11-15",
                question,
            )
            .await
        }
    };
    // an answer from memory is replaced
    let out = ask(
        "Jane Austen was born in 1775.",
        "In what year was she born?",
    )
    .await;
    assert!(!out.contains("1775"), "{out}");
    assert!(out.contains("I could not find a verified answer"), "{out}");
    assert!(out.contains("ℹ No sources were found"), "{out}");
    let pl = ask(
        "Jane Austen urodziła się w 1775 roku.",
        "W którym roku się urodziła Jane Austen?",
    )
    .await;
    assert!(!pl.contains("1775") && pl.contains("Nie znalazłem"), "{pl}");
    // a refusal is kept as the model wrote it
    let out = ask(
        "I could not find anything about that café.",
        "What is its phone number?",
    )
    .await;
    assert!(
        out.starts_with("I could not find anything about that café."),
        "{out}"
    );
}

#[allow(clippy::await_holding_lock)]
#[quench_test::test]
async fn gather_adds_wikipedia_articles_to_the_web_results() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (server, switchboard) = mock_estate(&[]).await;
    on_chat(
        &server,
        "retrieval planner",
        completion(r#"{"kind":"factual","queries":["Frank Herbert death date"]}"#),
    )
    .await;
    let mut registry = registry(&results(&[(
        "Currency rates for the Swiss franc today.",
        "https://kantor.example/franc",
    )]));
    registry.register(
        "wikipedia".to_string(),
        Box::new(Search(results(&[(
            "Frank Herbert: Herbert (October 8, 1920 - February 11, 1986) was an American author.",
            "https://en.wikipedia.org/wiki/Frank_Herbert",
        )]))),
    );
    let mut cfg = config();
    cfg.wikipedia = true;
    let g = gather(
        &cfg,
        &switchboard,
        &VllmClient::new(),
        &registry,
        "searxng",
        &answerer(&server),
        &[],
        "When did Frank Herbert die?",
        "2026-11-15",
        None,
    )
    .await
    .expect("grounded");
    assert!(g.block.contains("February 11, 1986"), "{}", g.block);
    assert!(g.sources.iter().any(|s| s.domain == "en.wikipedia.org"));

    // switched off, the same search never asks Wikipedia
    cfg.wikipedia = false;
    let g = gather(
        &cfg,
        &switchboard,
        &VllmClient::new(),
        &registry,
        "searxng",
        &answerer(&server),
        &[],
        "When did Frank Herbert die?",
        "2026-11-15",
        None,
    )
    .await
    .expect("grounded");
    assert!(!g.block.contains("February 11, 1986"), "{}", g.block);
}

/// The embeddings prefer the "330 metres" snippet, so it is document 0 of the rerank request; the
/// reranker (when it answers) prefers document 1, the Eiffel one.
async fn gather_with_reranker(rerank: ResponseTemplate, model: &str) -> Grounding {
    let (server, switchboard) = mock_estate(&["BAAI/bge-reranker-v2-m3"]).await;
    on_chat(
        &server,
        "retrieval planner",
        completion(r#"{"kind":"factual","queries":["height of the tower"]}"#),
    )
    .await;
    Mock::given(method("POST"))
        .and(path("/v1/embeddings"))
        .respond_with(json(
            200,
            &quench_serde::json!({"data": [
                {"index": 0, "embedding": [1.0, 0.0]},
                {"index": 1, "embedding": [0.6, 0.8]},
                {"index": 2, "embedding": [1.0, 0.1]},
            ]}),
        ))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/rerank"))
        .respond_with(rerank)
        .mount(&server)
        .await;
    let mut cfg = config();
    cfg.passages = 1;
    cfg.rerank_model = model.to_string();
    gather(
        &cfg,
        &switchboard,
        &VllmClient::new(),
        &registry(&results(&[
            (
                "The tower was designed by Gustave Eiffel.",
                "https://a.example/a",
            ),
            ("The tower is 330 metres high.", "https://b.example/b"),
        ])),
        "searxng",
        &answerer(&server),
        &[],
        "How high is the tower?",
        "2026-11-15",
        None,
    )
    .await
    .expect("grounded")
}

#[allow(clippy::await_holding_lock)]
#[quench_test::test]
async fn gather_lets_the_reranker_overrule_the_embedding_order() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let g = gather_with_reranker(
        json(
            200,
            &quench_serde::json!({"results": [
                {"index": 0, "relevance_score": 0.2},
                {"index": 1, "relevance_score": 0.9},
            ]}),
        ),
        "BAAI/bge-reranker-v2-m3",
    )
    .await;
    assert!(g.block.contains("Gustave Eiffel"), "{}", g.block);
    assert!(!g.block.contains("330 metres"), "{}", g.block);
}

#[allow(clippy::await_holding_lock)]
#[quench_test::test]
async fn gather_keeps_the_embedding_order_when_the_reranker_fails_or_is_off() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let failing = gather_with_reranker(
        json(500, &quench_serde::json!({})),
        "BAAI/bge-reranker-v2-m3",
    )
    .await;
    assert!(failing.block.contains("330 metres"), "{}", failing.block);
    let off = gather_with_reranker(
        json(
            200,
            &quench_serde::json!({"results": [
                {"index": 0, "relevance_score": 0.2},
                {"index": 1, "relevance_score": 0.9},
            ]}),
        ),
        "",
    )
    .await;
    assert!(off.block.contains("330 metres"), "{}", off.block);
}
