//! Unit tests for `files/extractor.rs`.

use sage_service::files::extractor::*;

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
