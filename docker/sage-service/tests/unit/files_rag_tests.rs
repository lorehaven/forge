//! `files/rag.rs`: the prompt addition built from a conversation's files and the excerpts found
//! for the message. The search itself is SQL on Postgres; what is tested here is everything
//! around it - configuration, the query wrapper, file visibility and the excerpt budget.

use crate::env_support::env_lock;
use crate::mock_estate::mock_estate;
use quench_db::prelude::{Crud, Db};
use sage_service::clients::vllm::VllmClient;
use sage_service::domain::models::{Conversation, File};
use sage_service::files::rag::*;

fn hit(file_name: &str, content: &str, similarity: f64, detail: Option<&str>) -> ChunkHit {
    ChunkHit {
        file_id: "f".to_string(),
        chunk_id: "c".to_string(),
        file_name: file_name.to_string(),
        chunk_index: 0,
        content: content.to_string(),
        similarity,
        detail: detail.map(str::to_string),
    }
}

const fn config(threshold: f64, max_chars: usize) -> RagConfig {
    RagConfig {
        auto_inject: true,
        top_k: 4,
        similarity_threshold: threshold,
        max_context_chars: max_chars,
    }
}

#[quench_test::test]
fn detail_comes_from_the_heading_then_the_page() {
    let heading = quench_serde::json!({"heading": "Install", "page": 3});
    assert_eq!(
        detail_from_metadata(Some(&heading)).as_deref(),
        Some("Install")
    );
    let page = quench_serde::json!({"page": 3});
    assert_eq!(detail_from_metadata(Some(&page)).as_deref(), Some("p.3"));
    let neither = quench_serde::json!({"other": 1});
    assert_eq!(detail_from_metadata(Some(&neither)), None);
    let not_an_object = quench_serde::json!([1, 2]);
    assert_eq!(detail_from_metadata(Some(&not_an_object)), None);
    assert_eq!(detail_from_metadata(None), None);
}

#[allow(clippy::await_holding_lock)] // env vars are process-global
#[quench_test::test]
fn the_configuration_has_defaults_and_reads_the_environment() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let vars = [
        "SAGE_RAG_AUTO_INJECT",
        "SAGE_RAG_TOP_K",
        "SAGE_RAG_SIMILARITY_THRESHOLD",
        "SAGE_RAG_MAX_CONTEXT_CHARS",
        "SAGE_EMBEDDING_QUERY_INSTRUCTION",
    ];
    for var in vars {
        quench_core::env::remove(var);
    }
    let defaults = RagConfig::from_env();
    assert!(defaults.auto_inject);
    assert_eq!(defaults.top_k, 4);
    assert!((defaults.similarity_threshold - 0.35).abs() < f64::EPSILON);
    assert_eq!(defaults.max_context_chars, 2000);
    assert!(query_input("how?").starts_with("Instruct: Given a search query"));
    assert!(query_input("how?").ends_with("\nQuery: how?"));

    quench_core::env::set("SAGE_RAG_AUTO_INJECT", "false");
    quench_core::env::set("SAGE_RAG_TOP_K", "9");
    quench_core::env::set("SAGE_RAG_SIMILARITY_THRESHOLD", "0.8");
    quench_core::env::set("SAGE_RAG_MAX_CONTEXT_CHARS", "100");
    quench_core::env::set("SAGE_EMBEDDING_QUERY_INSTRUCTION", "Find it");
    let custom = RagConfig::from_env();
    assert!(!custom.auto_inject);
    assert_eq!(custom.top_k, 9);
    assert!((custom.similarity_threshold - 0.8).abs() < f64::EPSILON);
    assert_eq!(custom.max_context_chars, 100);
    assert_eq!(query_input("how?"), "Instruct: Find it\nQuery: how?");

    // an empty instruction embeds the query verbatim, for models without one
    quench_core::env::set("SAGE_EMBEDDING_QUERY_INSTRUCTION", "  ");
    assert_eq!(query_input("how?"), "how?");
    // an unparseable threshold falls back to the default
    quench_core::env::set("SAGE_RAG_SIMILARITY_THRESHOLD", "high");
    assert!((RagConfig::from_env().similarity_threshold - 0.35).abs() < f64::EPSILON);

    for var in vars {
        quench_core::env::remove(var);
    }
}

#[quench_test::test]
fn only_hits_above_the_threshold_are_quoted_with_their_location() {
    let mut out = String::from("prefix");
    let injected = append_excerpts(
        &mut out,
        vec![
            hit("guide.pdf", "Install with cargo.", 0.9, Some("p.2")),
            hit("faq.md", "Unrelated.", 0.1, None),
            hit("notes.txt", "Exactly at the bar.", 0.5, None),
        ],
        &config(0.5, 1000),
    );
    assert_eq!(injected.len(), 2);
    assert!(out.starts_with("prefix\nRELEVANT FILE EXCERPTS\n"), "{out}");
    assert!(
        out.contains("[guide.pdf · p.2 · similarity 0.90]\nInstall with cargo.\n"),
        "{out}"
    );
    assert!(
        out.contains("[notes.txt · similarity 0.50]\nExactly at the bar.\n"),
        "{out}"
    );
    assert!(!out.contains("Unrelated"), "{out}");
}

#[quench_test::test]
fn nothing_is_added_when_no_hit_is_relevant() {
    let mut out = String::from("prefix");
    let injected = append_excerpts(&mut out, vec![hit("a", "x", 0.1, None)], &config(0.5, 1000));
    assert!(injected.is_empty());
    assert_eq!(out, "prefix");
    assert!(append_excerpts(&mut out, Vec::new(), &config(0.0, 1000)).is_empty());
    assert_eq!(out, "prefix");
}

#[quench_test::test]
fn excerpts_stop_at_the_character_budget_without_splitting_a_character() {
    let mut out = String::new();
    // each "ł" is two bytes, so a cut at byte 5 would fall inside one
    let injected = append_excerpts(
        &mut out,
        vec![
            hit("a", "łłłłłłłłł", 0.9, None),
            hit("b", "second", 0.8, None),
            hit("c", "third", 0.7, None),
        ],
        &config(0.0, 5),
    );
    // 4 of the 5 bytes go to the first excerpt (two whole characters), the last byte to the next
    assert_eq!(injected.len(), 2);
    assert!(out.contains("łł\n"), "{out}");
    assert!(!out.contains("łłł"), "{out}");
    assert!(out.contains("[b · similarity 0.80]\ns\n"), "{out}");
    assert!(!out.contains("second") && !out.contains("third"), "{out}");

    // an exhausted budget stops the list
    let mut out = String::new();
    let injected = append_excerpts(
        &mut out,
        vec![hit("a", "łłłł", 0.9, None), hit("b", "second", 0.8, None)],
        &config(0.0, 4),
    );
    assert_eq!(injected.len(), 1);
    assert!(!out.contains("second"), "{out}");

    // a budget that lasts for two excerpts
    let mut out = String::new();
    let injected = append_excerpts(
        &mut out,
        vec![
            hit("a", "12345", 0.9, None),
            hit("b", "abcdefgh", 0.8, None),
        ],
        &config(0.0, 8),
    );
    assert_eq!(injected.len(), 2);
    assert!(out.contains("12345") && out.contains("abc\n"), "{out}");
    assert!(!out.contains("abcd"), "{out}");
}

fn seeded_file(id: &str, conversation: &str, name: &str, mime: &str, status: &str) -> File {
    File {
        id: id.to_string(),
        owner: "admin".to_string(),
        file_name: name.to_string(),
        mime_type: mime.to_string(),
        file_size: 10,
        conversation_id: Some(conversation.to_string()),
        project_id: None,
        message_id: None,
        status: status.to_string(),
        error_message: None,
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
    }
}

async fn conversation(db: &Db, id: &str) {
    db.repository::<Conversation>()
        .create(&Conversation {
            id: id.to_string(),
            title: "t".to_string(),
            active_message_id: None,
            owner: "admin".to_string(),
            project_id: None,
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        })
        .await
        .unwrap();
}

#[allow(clippy::await_holding_lock)]
#[quench_test::test]
async fn the_prompt_addition_lists_ready_documents_and_skips_the_rest() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (_server, switchboard) = mock_estate(&[]).await;
    let vllm = VllmClient::new();
    let db = Db::InMemory(quench_db::InMemoryDb::new());

    // no such conversation
    assert!(
        augment_system_prompt(&db, &switchboard, &vllm, "c1", "hi")
            .await
            .is_none()
    );
    // a conversation without files
    conversation(&db, "c1").await;
    assert!(
        augment_system_prompt(&db, &switchboard, &vllm, "c1", "hi")
            .await
            .is_none()
    );

    let files = db.repository::<File>();
    files
        .create(&seeded_file("1", "c1", "photo.png", "image/png", "ready"))
        .await
        .unwrap();
    files
        .create(&seeded_file(
            "2",
            "c1",
            "busy.pdf",
            "application/pdf",
            "processing",
        ))
        .await
        .unwrap();
    // images and unfinished files do not count
    assert!(
        augment_system_prompt(&db, &switchboard, &vllm, "c1", "hi")
            .await
            .is_none()
    );

    files
        .create(&seeded_file("3", "c1", "notes.txt", "text/plain", "ready"))
        .await
        .unwrap();
    files
        .create(&seeded_file(
            "4",
            "other",
            "elsewhere.txt",
            "text/plain",
            "ready",
        ))
        .await
        .unwrap();
    let (text, hits) =
        augment_system_prompt(&db, &switchboard, &vllm, "c1", "what is in my notes?")
            .await
            .expect("a ready document");
    assert!(text.contains("UPLOADED FILES"), "{text}");
    assert!(text.contains("- notes.txt\n"), "{text}");
    assert!(
        !text.contains("photo.png") && !text.contains("busy.pdf"),
        "{text}"
    );
    assert!(!text.contains("elsewhere.txt"), "{text}");
    // the excerpt search needs Postgres, so the list stands alone
    assert!(hits.is_empty());
    assert!(!text.contains("RELEVANT FILE EXCERPTS"));
}

#[allow(clippy::await_holding_lock)]
#[quench_test::test]
async fn auto_injection_can_be_switched_off_and_needs_a_message() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (_server, switchboard) = mock_estate(&[]).await;
    let vllm = VllmClient::new();
    let db = Db::InMemory(quench_db::InMemoryDb::new());
    conversation(&db, "c1").await;
    db.repository::<File>()
        .create(&seeded_file("1", "c1", "notes.txt", "text/plain", "ready"))
        .await
        .unwrap();

    quench_core::env::set("SAGE_RAG_AUTO_INJECT", "false");
    let off = augment_system_prompt(&db, &switchboard, &vllm, "c1", "question").await;
    quench_core::env::remove("SAGE_RAG_AUTO_INJECT");
    assert!(off.expect("still lists the file").0.contains("- notes.txt"));

    let blank = augment_system_prompt(&db, &switchboard, &vllm, "c1", "   ").await;
    assert!(blank.expect("still lists the file").1.is_empty());
}

#[quench_test::test]
async fn the_sources_of_messages_and_the_web_are_not_stored_without_postgres() {
    let db = Db::InMemory(quench_db::InMemoryDb::new());
    assert!(
        record_rag_contexts(&db, "m", &[hit("a", "x", 0.9, None)], "auto")
            .await
            .is_ok()
    );
    assert!(load_sources_for_messages(&db, &[]).await.is_empty());
    assert!(
        load_sources_for_messages(&db, &["m".to_string()])
            .await
            .is_empty()
    );
    assert!(record_web_sources(&db, "m", &[]).await.is_ok());
}
