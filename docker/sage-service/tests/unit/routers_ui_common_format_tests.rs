//! Unit tests for `routers/ui/common/format.rs`.

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
