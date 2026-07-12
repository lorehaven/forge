use crate::context_manager::TokenCounter;
use crate::files::extractor::Segment;
use serde_json::{Map, Value};

pub struct ChunkerConfig {
    pub max_tokens: u32,
    pub overlap_tokens: u32,
}

impl ChunkerConfig {
    pub fn from_env() -> Self {
        Self {
            max_tokens: envmnt::get_u64("SAGE_CHUNK_SIZE_TOKENS", 512) as u32,
            overlap_tokens: envmnt::get_u64("SAGE_CHUNK_OVERLAP_TOKENS", 50) as u32,
        }
    }
}

/// A chunk of text ready to embed, carrying the metadata of the segment it came
/// from (heading, page, etc.).
#[derive(Debug, Clone)]
pub struct Chunk {
    pub content: String,
    pub metadata: Map<String, Value>,
}

/// Chunk each segment independently so a chunk never spans two segments, and
/// tag every chunk with its segment's metadata.
pub fn chunk_segments(segments: &[Segment], config: &ChunkerConfig) -> Vec<Chunk> {
    let mut chunks = Vec::new();
    for segment in segments {
        for content in chunk_text(&segment.text, config) {
            chunks.push(Chunk {
                content,
                metadata: segment.metadata.clone(),
            });
        }
    }
    chunks
}

/// Split text into chunks of roughly `max_tokens`, preferring paragraph
/// boundaries. Consecutive chunks share an overlap so that context spanning
/// a boundary is not lost.
pub fn chunk_text(text: &str, config: &ChunkerConfig) -> Vec<String> {
    let max_tokens = config.max_tokens.max(1);
    let paragraphs: Vec<&str> = text
        .split("\n\n")
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .collect();

    let mut chunks = Vec::new();
    let mut current = String::new();

    for paragraph in paragraphs {
        for piece in split_oversized(paragraph, max_tokens) {
            let candidate_tokens =
                TokenCounter::count_tokens(&current) + TokenCounter::count_tokens(&piece);
            if !current.is_empty() && candidate_tokens > max_tokens {
                let overlap = tail_overlap(&current, config.overlap_tokens);
                chunks.push(std::mem::take(&mut current));
                current = overlap;
            }
            if !current.is_empty() {
                current.push_str("\n\n");
            }
            current.push_str(&piece);
        }
    }

    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

/// Break a paragraph that alone exceeds the chunk size into whitespace-aligned
/// pieces; smaller paragraphs pass through unchanged.
fn split_oversized(paragraph: &str, max_tokens: u32) -> Vec<String> {
    if TokenCounter::count_tokens(paragraph) <= max_tokens {
        return vec![paragraph.to_string()];
    }

    let max_chars = (max_tokens as usize) * 4;
    let mut pieces = Vec::new();
    let mut piece = String::new();

    for word in paragraph.split_whitespace() {
        if !piece.is_empty() && piece.len() + 1 + word.len() > max_chars {
            pieces.push(std::mem::take(&mut piece));
        }
        if !piece.is_empty() {
            piece.push(' ');
        }
        piece.push_str(word);
    }
    if !piece.is_empty() {
        pieces.push(piece);
    }
    pieces
}

/// The trailing portion of a chunk carried into the next one, cut at a
/// whitespace boundary and capped at `overlap_tokens`.
fn tail_overlap(chunk: &str, overlap_tokens: u32) -> String {
    if overlap_tokens == 0 {
        return String::new();
    }
    let max_chars = (overlap_tokens as usize) * 4;
    if chunk.len() <= max_chars {
        return chunk.to_string();
    }
    let mut start = chunk.len() - max_chars;
    while !chunk.is_char_boundary(start) {
        start += 1;
    }
    let tail = &chunk[start..];
    match tail.find(char::is_whitespace) {
        Some(pos) => tail[pos..].trim_start().to_string(),
        None => tail.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        use crate::files::extractor::Segment;
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
        // The second segment's single chunk keeps its page metadata and never
        // mixes with the first segment's heading.
        let paged: Vec<_> = chunks
            .iter()
            .filter(|c| c.metadata.get("page") == Some(&json!(2)))
            .collect();
        assert_eq!(paged.len(), 1);
        assert!(paged[0].content.contains("second section"));
        assert!(!paged[0].metadata.contains_key("heading"));
    }
}
