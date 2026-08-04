//! Unit tests for `files/chunker.rs`.

use sage_service::domain::context::TokenCounter;
use sage_service::files::chunker::*;

fn config(max_tokens: u32, overlap_tokens: u32) -> ChunkerConfig {
    ChunkerConfig {
        max_tokens,
        overlap_tokens,
    }
}

#[test]
fn test_short_text_single_chunk() {
    let chunks = chunk_text("Hello world.\n\nSecond paragraph.", &config(512, 50));
    assert_eq!(chunks.len(), 1);
    assert!(chunks[0].contains("Hello world."));
    assert!(chunks[0].contains("Second paragraph."));
}

#[test]
fn test_empty_text() {
    assert!(chunk_text("", &config(512, 50)).is_empty());
    assert!(chunk_text("\n\n\n\n", &config(512, 50)).is_empty());
}

#[test]
fn test_splits_on_paragraphs() {
    // Each paragraph is ~25 tokens; max 30 forces one paragraph per chunk.
    let paragraph = "word ".repeat(20);
    let text = format!("{}\n\n{}", paragraph.trim(), paragraph.trim());
    let chunks = chunk_text(&text, &config(30, 0));
    assert_eq!(chunks.len(), 2);
}

#[test]
fn test_oversized_paragraph_is_split() {
    let text = "word ".repeat(1000);
    let chunks = chunk_text(text.trim(), &config(100, 0));
    assert!(chunks.len() > 1);
    for chunk in &chunks {
        assert!(TokenCounter::count_tokens(chunk) <= 110);
    }
}

#[test]
fn test_overlap_carried_between_chunks() {
    let paragraph_a = "alpha ".repeat(20);
    let paragraph_b = "beta ".repeat(20);
    let text = format!("{}\n\n{}", paragraph_a.trim(), paragraph_b.trim());
    let chunks = chunk_text(&text, &config(30, 10));
    assert_eq!(chunks.len(), 2);
    // The second chunk starts with the tail of the first.
    assert!(chunks[1].starts_with("alpha"));
    assert!(chunks[1].contains("beta"));
}

#[test]
fn test_all_content_preserved() {
    let text = (0..50)
        .map(|i| format!("Sentence number {} with some padding words.", i))
        .collect::<Vec<_>>()
        .join("\n\n");
    let chunks = chunk_text(&text, &config(60, 10));
    let combined = chunks.join(" ");
    for i in 0..50 {
        assert!(combined.contains(&format!("Sentence number {}", i)));
    }
}

#[test]
fn test_chunk_segments_carry_metadata() {
    use sage_service::files::extractor::Segment;
    use serde_json::json;

    let big = "word ".repeat(200);
    let segments = vec![
        Segment {
            text: format!("# Intro\n\n{}", big.trim()),
            metadata: serde_json::Map::from_iter([("heading".to_string(), json!("Intro"))]),
        },
        Segment {
            text: "short second section".to_string(),
            metadata: serde_json::Map::from_iter([("page".to_string(), json!(2))]),
        },
    ];
    let chunks = chunk_segments(&segments, &config(50, 10));
    // The oversized first segment yields multiple chunks, all tagged Intro.
    let intro: Vec<_> = chunks
        .iter()
        .filter(|c| c.metadata.get("heading") == Some(&json!("Intro")))
        .collect();
    assert!(intro.len() > 1);
    // The second segment's single chunk keeps its page metadata and never mixes with the first segment's heading.
    let paged: Vec<_> = chunks
        .iter()
        .filter(|c| c.metadata.get("page") == Some(&json!(2)))
        .collect();
    assert_eq!(paged.len(), 1);
    assert!(paged[0].content.contains("second section"));
    assert!(!paged[0].metadata.contains_key("heading"));
}
