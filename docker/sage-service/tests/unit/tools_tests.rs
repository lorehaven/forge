//! Unit tests for `tools/mod.rs`.

use sage_service::tools::*;

#[test]
fn test_tool_definitions_valid_json() {
    let defs = get_tool_definitions_for_prompt();
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(&defs);
    assert!(parsed.is_ok(), "Tool definitions should be valid JSON");
}

#[test]
fn extract_rich_text_keeps_tables_lists_and_json_ld_and_drops_chrome() {
    use sage_service::tools::web_fetch::extract_rich_text;
    let html = r#"<html><head><title>Results 2026</title>
        <meta name="description" content="Official race results for the 2026 season.">
        <script type="application/ld+json">{"@type":"SportsEvent","name":"Grand Prix","startDate":"2026-09-20"}</script>
        </head><body>
        <nav><ul><li>Home page navigation link</li></ul></nav>
        <main><h1>Race results</h1>
        <table><tr><th>Pos</th><th>Driver</th></tr><tr><td>1</td><td>Max Verstappen</td></tr></table>
        <ul><li>Fastest lap by someone quick</li></ul>
        <p>This paragraph is long enough to be kept in the extracted text output.</p></main>
        <footer><p>Footer text that should never be quoted in answers.</p></footer>
        </body></html>"#;
    let text = extract_rich_text(html, 10_000).unwrap();
    assert!(text.contains("# Results 2026"));
    assert!(text.contains("Official race results"));
    assert!(text.contains("name: Grand Prix"));
    assert!(text.contains("startDate: 2026-09-20"));
    assert!(text.contains("Pos | Driver"));
    assert!(text.contains("1 | Max Verstappen"));
    assert!(text.contains("- Fastest lap by someone quick"));
    assert!(text.contains("This paragraph is long enough"));
    assert!(!text.contains("navigation link"));
    assert!(!text.contains("Footer text"));
}

#[test]
fn extract_rich_text_truncates_on_char_boundary_and_rejects_empty_pages() {
    use sage_service::tools::web_fetch::extract_rich_text;
    let long = format!(
        "<html><body><p>{}</p></body></html>",
        "zażółć gęślą jaźń ".repeat(200)
    );
    let text = extract_rich_text(&long, 100).unwrap();
    assert!(text.ends_with("[Content truncated...]"));
    assert!(extract_rich_text("<html><body></body></html>", 100).is_err());
}
