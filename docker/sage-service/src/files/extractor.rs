use serde_json::{Map, Value};

/// A contiguous piece of a document together with metadata describing where it
/// came from (a Markdown heading, a PDF page, etc.). Chunks never span two
/// segments, so every chunk inherits exactly one segment's metadata.
#[derive(Debug, Clone)]
pub struct Segment {
    pub text: String,
    pub metadata: Map<String, Value>,
}

impl Segment {
    fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            metadata: Map::new(),
        }
    }

    fn with(text: impl Into<String>, key: &str, value: Value) -> Self {
        let mut metadata = Map::new();
        metadata.insert(key.to_string(), value);
        Self {
            text: text.into(),
            metadata,
        }
    }
}

/// Extract a document into labelled segments based on its MIME type.
pub fn extract_segments(mime_type: &str, data: &[u8]) -> Result<Vec<Segment>, String> {
    let segments = match mime_type {
        "application/pdf" => extract_pdf(data)?,
        "text/csv" => vec![extract_csv(data)?],
        "text/markdown" => extract_markdown(&String::from_utf8_lossy(data)),
        "text/html" => vec![extract_html(&String::from_utf8_lossy(data))],
        other if is_plain_text_mime(other) => {
            let text = String::from_utf8_lossy(data).into_owned();
            match language_for_mime(other) {
                Some(lang) => vec![Segment::with(text, "language", Value::from(lang))],
                None => vec![Segment::new(text)],
            }
        }
        other => return Err(format!("Unsupported MIME type for extraction: {}", other)),
    };

    let segments: Vec<Segment> = segments
        .into_iter()
        .filter(|s| !s.text.trim().is_empty())
        .collect();
    if segments.is_empty() {
        return Err("No text content could be extracted from the file".to_string());
    }
    Ok(segments)
}

/// Convenience wrapper returning the full plain text of a document.
pub fn extract_text(mime_type: &str, data: &[u8]) -> Result<String, String> {
    Ok(extract_segments(mime_type, data)?
        .into_iter()
        .map(|s| s.text)
        .collect::<Vec<_>>()
        .join("\n\n"))
}

/// MIME types ingested verbatim as plain text: everything textual plus the
/// structured formats whose raw source is already readable (JSON, YAML, ...).
/// `text/csv`, `text/markdown` and `text/html` are matched before this guard
/// and get dedicated extractors.
fn is_plain_text_mime(mime: &str) -> bool {
    mime.starts_with("text/")
        || matches!(
            mime,
            "application/json"
                | "application/yaml"
                | "application/toml"
                | "application/xml"
                | "application/sql"
        )
}

/// Language label for source code and structured data MIME types, used to tag
/// chunks so retrieval hits can name the language. None for generic text.
fn language_for_mime(mime: &str) -> Option<&'static str> {
    Some(match mime {
        "application/json" => "json",
        "application/yaml" => "yaml",
        "application/toml" => "toml",
        "application/xml" => "xml",
        "application/sql" => "sql",
        "text/x-rust" => "rust",
        "text/x-python" => "python",
        "text/javascript" => "javascript",
        "text/x-typescript" => "typescript",
        "text/x-java" => "java",
        "text/x-kotlin" => "kotlin",
        "text/x-go" => "go",
        "text/x-c" => "c",
        "text/x-c++" => "cpp",
        "text/x-csharp" => "csharp",
        "text/x-ruby" => "ruby",
        "text/x-php" => "php",
        "text/x-swift" => "swift",
        "text/x-shellscript" => "shell",
        "text/css" => "css",
        _ => return None,
    })
}

fn extract_pdf(data: &[u8]) -> Result<Vec<Segment>, String> {
    let pages = pdf_extract::extract_text_from_mem_by_pages(data)
        .map_err(|e| format!("PDF text extraction failed: {}", e))?;
    Ok(pages
        .into_iter()
        .enumerate()
        .map(|(i, text)| Segment::with(text, "page", Value::from(i as i64 + 1)))
        .collect())
}

/// Render CSV rows as "header: value" lines so each record stays readable and
/// self-describing after chunking.
fn extract_csv(data: &[u8]) -> Result<Segment, String> {
    let mut reader = csv::ReaderBuilder::new().flexible(true).from_reader(data);

    let headers: Vec<String> = reader
        .headers()
        .map_err(|e| format!("CSV parsing failed: {}", e))?
        .iter()
        .map(|h| h.trim().to_string())
        .collect();

    let mut out = String::new();
    for (row_idx, record) in reader.records().enumerate() {
        let record =
            record.map_err(|e| format!("CSV parsing failed at row {}: {}", row_idx + 1, e))?;
        let line: Vec<String> = record
            .iter()
            .enumerate()
            .map(|(i, value)| match headers.get(i) {
                Some(h) if !h.is_empty() => format!("{}: {}", h, value.trim()),
                _ => value.trim().to_string(),
            })
            .collect();
        out.push_str(&line.join(", "));
        out.push('\n');
    }

    if out.trim().is_empty() {
        return Err("CSV file contains no data rows".to_string());
    }

    let mut segment = Segment::new(out);
    segment
        .metadata
        .insert("format".to_string(), Value::from("csv"));
    Ok(segment)
}

/// Flatten an HTML document into readable plain text: block-level elements
/// become paragraphs (so the chunker can split on `\n\n`), inline markup is
/// dropped, and non-content subtrees (scripts, styles, head) are skipped.
/// The document title, when present, is kept as segment metadata.
fn extract_html(html: &str) -> Segment {
    use scraper::{Html, Node, Selector};

    /// Elements whose text starts a new paragraph in the flattened output.
    fn is_block(tag: &str) -> bool {
        matches!(
            tag,
            "p" | "div"
                | "section"
                | "article"
                | "header"
                | "footer"
                | "main"
                | "aside"
                | "nav"
                | "h1"
                | "h2"
                | "h3"
                | "h4"
                | "h5"
                | "h6"
                | "ul"
                | "ol"
                | "li"
                | "table"
                | "tr"
                | "blockquote"
                | "pre"
                | "figure"
                | "figcaption"
                | "br"
                | "hr"
        )
    }

    /// Elements whose entire subtree carries no readable content.
    fn is_skipped(tag: &str) -> bool {
        matches!(tag, "script" | "style" | "noscript" | "template" | "head")
    }

    let document = Html::parse_document(html);

    let title = Selector::parse("title")
        .ok()
        .and_then(|s| document.select(&s).next())
        .map(|t| t.text().collect::<String>().trim().to_string())
        .filter(|t| !t.is_empty());

    let mut out = String::new();
    // Whether the source had whitespace between the last emitted text and the
    // next one; inline markup ("a<b>b</b>") must not introduce spaces.
    let mut pending_space = false;
    for node in document.root_element().descendants() {
        match node.value() {
            Node::Element(el)
                if is_block(el.name()) && !out.is_empty() && !out.ends_with("\n\n") =>
            {
                out.push_str("\n\n");
                pending_space = false;
            }
            Node::Text(text) => {
                let skipped = node
                    .ancestors()
                    .any(|a| matches!(a.value(), Node::Element(el) if is_skipped(el.name())));
                if skipped {
                    continue;
                }
                let at_break = out.is_empty() || out.ends_with("\n\n");
                let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
                if collapsed.is_empty() {
                    // Whitespace-only node still separates its neighbours.
                    pending_space |= !at_break;
                    continue;
                }
                if !at_break && (pending_space || text.starts_with(char::is_whitespace)) {
                    out.push(' ');
                }
                out.push_str(&collapsed);
                pending_space = text.ends_with(char::is_whitespace);
            }
            _ => {}
        }
    }

    match title {
        Some(t) => Segment::with(out, "title", Value::from(t)),
        None => Segment::new(out),
    }
}

/// Split Markdown into segments delimited by ATX headings (`#` .. `######`),
/// tagging each with its nearest heading so retrieval hits can name a section.
fn extract_markdown(text: &str) -> Vec<Segment> {
    let mut segments = Vec::new();
    let mut current_heading: Option<String> = None;
    let mut buffer = String::new();

    let flush = |segments: &mut Vec<Segment>, heading: &Option<String>, buffer: &str| {
        if buffer.trim().is_empty() {
            return;
        }
        match heading {
            Some(h) => segments.push(Segment::with(
                buffer.trim_end().to_string(),
                "heading",
                Value::from(h.clone()),
            )),
            None => segments.push(Segment::new(buffer.trim_end().to_string())),
        }
    };

    for line in text.lines() {
        if let Some(heading) = parse_heading(line) {
            flush(&mut segments, &current_heading, &buffer);
            buffer.clear();
            current_heading = Some(heading);
            buffer.push_str(line);
            buffer.push('\n');
        } else {
            buffer.push_str(line);
            buffer.push('\n');
        }
    }
    flush(&mut segments, &current_heading, &buffer);

    if segments.is_empty() {
        segments.push(Segment::new(text.to_string()));
    }
    segments
}

/// The text of an ATX heading line (`## Title` -> "Title"), or None.
fn parse_heading(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    if !trimmed.starts_with('#') {
        return None;
    }
    let hashes = trimmed.chars().take_while(|c| *c == '#').count();
    if hashes == 0 || hashes > 6 {
        return None;
    }
    let rest = &trimmed[hashes..];
    // A real heading needs a space after the hashes (`#foo` is not a heading).
    if !rest.starts_with(' ') {
        return None;
    }
    let title = rest.trim().trim_end_matches('#').trim();
    (!title.is_empty()).then(|| title.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_txt() {
        let segments = extract_segments("text/plain", b"hello world").unwrap();
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].text, "hello world");
        assert!(segments[0].metadata.is_empty());
    }

    #[test]
    fn test_extract_markdown_headings() {
        let md = "# Intro\n\nIntro body.\n\n## Details\n\nDetail body.";
        let segments = extract_segments("text/markdown", md.as_bytes()).unwrap();
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].metadata.get("heading").unwrap(), "Intro");
        assert!(segments[0].text.contains("Intro body."));
        assert_eq!(segments[1].metadata.get("heading").unwrap(), "Details");
        assert!(segments[1].text.contains("Detail body."));
    }

    #[test]
    fn test_markdown_preamble_without_heading() {
        let md = "Loose text.\n\n# Section\n\nBody.";
        let segments = extract_segments("text/markdown", md.as_bytes()).unwrap();
        assert_eq!(segments.len(), 2);
        assert!(segments[0].metadata.get("heading").is_none());
        assert_eq!(segments[1].metadata.get("heading").unwrap(), "Section");
    }

    #[test]
    fn test_parse_heading_variants() {
        assert_eq!(parse_heading("## Title").as_deref(), Some("Title"));
        assert_eq!(parse_heading("### Title ###").as_deref(), Some("Title"));
        assert_eq!(parse_heading("#nospace"), None);
        assert_eq!(parse_heading("plain"), None);
        assert_eq!(parse_heading("####### too many"), None);
    }

    #[test]
    fn test_extract_csv() {
        let segments = extract_segments("text/csv", b"name,value\nalpha,1\nbeta,2\n").unwrap();
        assert_eq!(segments.len(), 1);
        assert_eq!(
            segments[0].text,
            "name: alpha, value: 1\nname: beta, value: 2\n"
        );
        assert_eq!(segments[0].metadata.get("format").unwrap(), "csv");
    }

    #[test]
    fn test_extract_csv_empty() {
        assert!(extract_segments("text/csv", b"name,value\n").is_err());
    }

    #[test]
    fn test_extract_unknown_mime() {
        assert!(extract_segments("application/zip", b"x").is_err());
    }

    #[test]
    fn test_extract_json_tags_language() {
        let segments = extract_segments("application/json", br#"{"key": "value"}"#).unwrap();
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].text, r#"{"key": "value"}"#);
        assert_eq!(segments[0].metadata.get("language").unwrap(), "json");
    }

    #[test]
    fn test_extract_source_code_tags_language() {
        let segments = extract_segments("text/x-rust", b"fn main() {}").unwrap();
        assert_eq!(segments[0].text, "fn main() {}");
        assert_eq!(segments[0].metadata.get("language").unwrap(), "rust");
    }

    #[test]
    fn test_extract_generic_text_has_no_language() {
        let segments = extract_segments("text/x-unknown-lang", b"anything").unwrap();
        assert!(segments[0].metadata.get("language").is_none());
    }

    #[test]
    fn test_extract_html() {
        let html = r#"<html><head><title>My Page</title>
            <style>body { color: red; }</style>
            <script>var hidden = 1;</script></head>
            <body><h1>Heading</h1><p>First <b>paragraph</b>.</p>
            <ul><li>item one</li><li>item two</li></ul></body></html>"#;
        let segments = extract_segments("text/html", html.as_bytes()).unwrap();
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].metadata.get("title").unwrap(), "My Page");
        assert_eq!(
            segments[0].text,
            "Heading\n\nFirst paragraph.\n\nitem one\n\nitem two"
        );
    }

    #[test]
    fn test_extract_html_without_title_or_content() {
        assert!(extract_segments("text/html", b"<script>1</script>").is_err());
    }

    #[test]
    fn test_extract_text_joins_segments() {
        let md = "# A\n\nfirst\n\n# B\n\nsecond";
        let text = extract_text("text/markdown", md.as_bytes()).unwrap();
        assert!(text.contains("first"));
        assert!(text.contains("second"));
    }
}
