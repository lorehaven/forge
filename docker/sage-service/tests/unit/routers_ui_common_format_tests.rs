//! Unit tests for `routers/ui/common/format.rs`.

use sage_service::files::rag::RagSource;
use sage_service::routers::ui::common::format::*;

#[test]
fn test_format_message_headers() {
    let input = "### Conclusion\nChoose the appropriate loop.";
    let output = format_message(input);
    assert!(output.contains("<h3>Conclusion</h3>"));
    assert!(output.contains("<p>Choose the appropriate loop.</p>"));
}

#[test]
fn test_format_message_lists() {
    let input = "- **Readability**: for loop\n- **Consistency**: codebase";
    let output = format_message(input);
    assert!(output.contains("<ul><li><strong>Readability</strong>: for loop</li><li><strong>Consistency</strong>: codebase</li></ul>"));
}

#[test]
fn test_format_message_lists_with_double_newlines() {
    let input = "- **Readability**: for loop\n\n- **Consistency**: codebase";
    let output = format_message(input);
    assert!(output.contains("<ul><li><strong>Readability</strong>: for loop</li><li><strong>Consistency</strong>: codebase</li></ul>"));
}

#[test]
fn test_format_message_inline_code() {
    let input = "Use `for` loops where possible.";
    let output = format_message(input);
    assert!(output.contains("<code class=\"inline-code\">for</code>"));
}

#[test]
fn test_format_message_bold_italic() {
    let input = "This is **bold** and _italic_ and `code` text.";
    let output = format_message(input);
    assert!(output.contains("<strong>bold</strong>"));
    assert!(output.contains("<em>italic</em>"));
    assert!(output.contains("<code class=\"inline-code\">code</code>"));
}

#[test]
fn format_message_renders_headers_from_h1_through_h6() {
    for level in 1..=6 {
        let hashes = "#".repeat(level);
        let output = format_message(&format!("{hashes} Title"));
        assert!(
            output.contains(&format!("<h{level}>Title</h{level}>")),
            "level {level}: {output}"
        );
    }
}

#[test]
fn format_message_seven_hashes_is_not_a_header() {
    let output = format_message("####### too many");
    assert!(!output.contains("<h"));
}

#[test]
fn format_message_renders_an_ordered_list() {
    let output = format_message("1. first\n2. second");
    assert!(output.contains("<ol><li>first</li><li>second</li></ol>"));
}

#[test]
fn format_message_renders_a_horizontal_rule() {
    let output = format_message("above\n\n---\n\nbelow");
    assert!(output.contains("<hr"));
}

#[test]
fn format_message_renders_a_markdown_table() {
    let input = "| Name | Age |\n|---|---|\n| Alice | 30 |\n| Bob | 25 |";
    let output = format_message(input);
    assert!(output.contains("<table>"));
    assert!(output.contains("<th>Name</th>"));
    assert!(output.contains("<th>Age</th>"));
    assert!(output.contains("<td>Alice</td>"));
    assert!(output.contains("<td>30</td>"));
    assert!(output.contains("<td>Bob</td>"));
}

#[test]
fn format_message_renders_a_link_opening_in_a_new_tab() {
    let output = format_message("See [the docs](https://example.com/docs) for more.");
    assert!(output.contains("<a href=\"https://example.com/docs\" target=\"_blank\""));
    assert!(output.contains("the docs</a>"));
}

#[test]
fn format_message_renders_a_fenced_code_block_with_its_language() {
    let input = "before\n```rust\nfn main() {}\n```\nafter";
    let output = format_message(input);
    assert!(output.contains("code-block"));
    assert!(output.contains("code-lang"));
    assert!(output.contains(">rust<"));
    assert!(output.contains("fn main() {}"));
}

#[test]
fn format_message_code_block_without_a_language_tag_defaults_to_code() {
    let input = "```\nplain text block\n```";
    let output = format_message(input);
    assert!(output.contains(">code<"));
    assert!(output.contains("plain text block"));
}

#[test]
fn format_message_html_escapes_table_cell_content() {
    let input = "| A |\n|---|\n| <script> |";
    let output = format_message(input);
    assert!(output.contains("&lt;script&gt;"));
    assert!(!output.contains("<script>"));
}

#[test]
fn format_message_underscore_bold_and_italic() {
    let output = format_message("__bold__ and _italic_");
    assert!(output.contains("<strong>bold</strong>"));
    assert!(output.contains("<em>italic</em>"));
}

#[test]
fn format_message_is_empty_for_blank_input() {
    assert_eq!(format_message(""), "");
    assert_eq!(format_message("   \n  "), "");
}

// ---------------------------------------------------------------------------
// render_sources
// ---------------------------------------------------------------------------

#[test]
fn render_sources_is_none_for_no_sources() {
    assert!(render_sources(&[]).is_none());
}

#[test]
fn render_sources_shows_the_detail_when_present() {
    let sources = vec![RagSource {
        file_name: "report.pdf".to_string(),
        chunk_index: Some(2),
        detail: Some("page 3".to_string()),
        similarity: Some(0.87),
    }];
    let html = render_sources(&sources).unwrap().render();
    assert!(html.contains("report.pdf"));
    assert!(html.contains("page 3"));
    assert!(html.contains("87%"));
    // Detail present takes precedence over the chunk-index fallback.
    assert!(!html.contains("chunk 2"));
}

#[test]
fn render_sources_falls_back_to_the_chunk_index_without_detail() {
    let sources = vec![RagSource {
        file_name: "notes.txt".to_string(),
        chunk_index: Some(5),
        detail: None,
        similarity: None,
    }];
    let html = render_sources(&sources).unwrap().render();
    assert!(html.contains("chunk 5"));
    assert!(!html.contains('%'));
}

#[test]
fn render_sources_lists_every_source() {
    let sources = vec![
        RagSource {
            file_name: "a.txt".to_string(),
            chunk_index: None,
            detail: None,
            similarity: None,
        },
        RagSource {
            file_name: "b.txt".to_string(),
            chunk_index: None,
            detail: None,
            similarity: None,
        },
    ];
    let html = render_sources(&sources).unwrap().render();
    assert!(html.contains("a.txt"));
    assert!(html.contains("b.txt"));
}
