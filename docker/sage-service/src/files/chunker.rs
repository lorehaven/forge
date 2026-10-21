use crate::domain::context::TokenCounter;
use crate::files::extractor::Segment;
use serde_json::{Map, Value};

#[derive(Debug)]
pub struct ChunkerConfig {
    pub max_tokens: u32,
    pub overlap_tokens: u32,
}

impl ChunkerConfig {
    #[must_use]
    pub fn from_env() -> Self {
        Self {
            max_tokens: u32::try_from(envmnt::get_u64("SAGE_CHUNK_SIZE_TOKENS", 512))
                .unwrap_or(u32::MAX),
            overlap_tokens: u32::try_from(envmnt::get_u64("SAGE_CHUNK_OVERLAP_TOKENS", 50))
                .unwrap_or(u32::MAX),
        }
    }
}

/// A chunk of text ready to embed, carrying the metadata of the segment it came from (heading, page, etc.).
#[derive(Debug, Clone)]
pub struct Chunk {
    pub content: String,
    pub metadata: Map<String, Value>,
}

/// Chunk each segment independently so a chunk never spans two segments, tagging every chunk with its segment's metadata.
#[must_use]
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

/// Split text into chunks of roughly `max_tokens`, preferring paragraph boundaries. Consecutive
/// chunks share an overlap so context spanning a boundary isn't lost.
#[must_use]
pub fn chunk_text(text: &str, config: &ChunkerConfig) -> Vec<String> {
    let max_tokens = config.max_tokens.max(1);
    let paragraphs: Vec<&str> = text
        .split("\n\n")
        .map(str::trim)
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

/// Break a paragraph that alone exceeds the chunk size into whitespace-aligned pieces; smaller paragraphs pass through unchanged.
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

/// The trailing portion of a chunk carried into the next one, cut at a whitespace boundary and capped at `overlap_tokens`.
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
    tail.find(char::is_whitespace).map_or_else(
        || tail.to_string(),
        |pos| tail[pos..].trim_start().to_string(),
    )
}
