use quench_web::prelude::*;
enum BlockState {
    None,
    Paragraph(Vec<String>),
    Table(Vec<String>),
    UnorderedList(Vec<String>),
    OrderedList(Vec<String>),
}

fn parse_header(line: &str) -> Option<(usize, String)> {
    let trimmed = line.trim();
    if !trimmed.starts_with('#') {
        return None;
    }
    let mut chars = trimmed.chars().peekable();
    let mut level = 0;
    while chars.peek() == Some(&'#') {
        level += 1;
        chars.next();
    }
    if level > 0 && level <= 6 && chars.peek() == Some(&' ') {
        chars.next(); // Consume space
        let content: String = chars.collect();
        Some((level, content.trim().to_string()))
    } else {
        None
    }
}

fn parse_unordered_list_item(line: &str) -> Option<String> {
    let trimmed = line.trim();
    trimmed
        .strip_prefix("- ")
        .or_else(|| trimmed.strip_prefix("* "))
        .or_else(|| trimmed.strip_prefix("+ "))
        .map(|rest| rest.trim().to_string())
}

fn parse_ordered_list_item(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let mut chars = trimmed.chars().peekable();
    if chars.peek().is_some_and(|c| c.is_ascii_digit()) {
        let mut num_str = String::new();
        while chars.peek().is_some_and(|c| c.is_ascii_digit()) {
            num_str.push(chars.next().unwrap());
        }
        if chars.peek() == Some(&'.') {
            chars.next(); // Consume '.'
            if chars.peek() == Some(&' ') {
                chars.next(); // Consume ' '
                let content: String = chars.collect();
                return Some(content.trim().to_string());
            }
        }
    }
    None
}

fn is_horizontal_rule(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.len() < 3 {
        return false;
    }
    let first_char = trimmed.chars().next().unwrap();
    if first_char != '-' && first_char != '*' && first_char != '_' {
        return false;
    }
    trimmed.chars().all(|c| c == first_char)
}

/// Render a "Sources" block listing the uploaded-file excerpts that fed an
/// assistant answer. Returns None when there are no sources.
pub fn render_sources(sources: &[crate::files::rag::RagSource]) -> Option<Element> {
    if sources.is_empty() {
        return None;
    }
    let mut list = div().class("message-sources");
    list = list.child(
        span()
            .class("message-sources-label")
            .child(i().class("fas fa-file-lines"))
            .child(span().attr("data-i18n", "ui_chat_sources").text("Sources")),
    );
    for source in sources {
        let mut item = div().class("message-source-item").text(&source.file_name);
        if let Some(detail) = &source.detail {
            item = item.child(span().text(format!(" · {}", detail)));
        } else if let Some(idx) = source.chunk_index {
            item = item.child(span().text(" · ")).child(
                span()
                    .attr("data-i18n", "ui_chat_source_chunk")
                    .attr(
                        "data-i18n-args",
                        serde_json::json!({ "index": idx }).to_string(),
                    )
                    .text(format!("chunk {}", idx)),
            );
        }
        if let Some(sim) = source.similarity {
            item = item.child(
                span()
                    .class("message-source-score")
                    .text(format!("{:.0}%", sim * 100.0)),
            );
        }
        list = list.child(item);
    }
    Some(list)
}

pub fn format_message(text: &str) -> String {
    let parts: Vec<&str> = text.split("```").collect();
    let mut html = String::new();

    for (i, part) in parts.iter().enumerate() {
        if i % 2 == 0 {
            // Text part
            let formatted = format_text_part(part);
            if !formatted.trim().is_empty() {
                html.push_str(&format!(
                    "<div class=\"message-content\">{}</div>",
                    formatted
                ));
            }
        } else {
            // Code part
            html.push_str(&format_code_part(part));
        }
    }
    html
}

fn format_text_part(text: &str) -> String {
    // First pass: extract tool result blocks and preserve them
    let mut text_parts = Vec::new();
    let mut current_pos = 0;

    while let Some(start) = text[current_pos..].find(r#"<div class="tool-result"#) {
        let abs_start = current_pos + start;
        // Add text before this tool block
        if abs_start > current_pos {
            text_parts.push((text[current_pos..abs_start].to_string(), false));
        }

        // Find the matching closing div by counting nesting
        let rest = &text[abs_start..];
        let mut depth = 0;
        let mut found_end = false;
        let mut pos = 0;

        // Use a safe iterator that respects char boundaries
        let mut search_pos = 0;
        while search_pos < rest.len() {
            if let Some(next_open) = rest[search_pos..].find("<div") {
                if let Some(next_close) = rest[search_pos..].find("</div>") {
                    if next_open < next_close {
                        // Found opening tag first
                        depth += 1;
                        search_pos += next_open + 4;
                    } else {
                        // Found closing tag first
                        depth -= 1;
                        search_pos += next_close + 6;
                        if depth == 0 {
                            pos = search_pos;
                            found_end = true;
                            break;
                        }
                    }
                } else {
                    // Only opening tag found
                    depth += 1;
                    search_pos += next_open + 4;
                }
            } else if let Some(next_close) = rest[search_pos..].find("</div>") {
                // Only closing tag found
                depth -= 1;
                search_pos += next_close + 6;
                if depth == 0 {
                    pos = search_pos;
                    found_end = true;
                    break;
                }
            } else {
                // No more tags
                break;
            }
        }

        if found_end {
            let tool_end = abs_start + pos;
            text_parts.push((text[abs_start..tool_end].to_string(), true));
            current_pos = tool_end;
        } else {
            // Malformed, just include rest as text
            text_parts.push((text[abs_start..].to_string(), false));
            break;
        }
    }

    // Add remaining text
    if current_pos < text.len() {
        text_parts.push((text[current_pos..].to_string(), false));
    }

    // Process each part
    let mut html = String::new();
    for (part, is_tool_block) in text_parts {
        if is_tool_block {
            html.push_str(&part);
        } else {
            html.push_str(&format_text_part_internal(&part));
        }
    }

    html
}

fn format_text_part_internal(text: &str) -> String {
    let mut html = String::new();
    let mut state = BlockState::None;

    let emit = |state: &mut BlockState, html: &mut String| {
        match state {
            BlockState::None => {}
            BlockState::Paragraph(lines) => {
                if !lines.is_empty() {
                    let content = lines.join(" ");
                    html.push_str(&format!("<p>{}</p>", format_inline(&html_escape(&content))));
                }
            }
            BlockState::Table(rows) => {
                if !rows.is_empty() {
                    let row_strs: Vec<&str> = rows.iter().map(|s| s.as_str()).collect();
                    html.push_str(&render_table(row_strs));
                }
            }
            BlockState::UnorderedList(items) => {
                if !items.is_empty() {
                    html.push_str("<ul>");
                    for item in items {
                        html.push_str(&format!("<li>{}</li>", format_inline(&html_escape(item))));
                    }
                    html.push_str("</ul>");
                }
            }
            BlockState::OrderedList(items) => {
                if !items.is_empty() {
                    html.push_str("<ol>");
                    for item in items {
                        html.push_str(&format!("<li>{}</li>", format_inline(&html_escape(item))));
                    }
                    html.push_str("</ol>");
                }
            }
        }
        *state = BlockState::None;
    };

    for line in text.lines() {
        let trimmed = line.trim();

        if trimmed.is_empty() {
            // Empty line: close Paragraph/Table.
            // We keep list active so consecutive list items separated by blank lines group together.
            match state {
                BlockState::Paragraph(_) | BlockState::Table(_) => {
                    emit(&mut state, &mut html);
                }
                _ => {}
            }
            continue;
        }

        // 1. Horizontal rule check
        if is_horizontal_rule(line) {
            emit(&mut state, &mut html);
            html.push_str("<hr>");
            continue;
        }

        // 2. Table check
        if trimmed.starts_with('|') && trimmed.ends_with('|') {
            match state {
                BlockState::Table(ref mut rows) => {
                    rows.push(line.to_string());
                }
                _ => {
                    emit(&mut state, &mut html);
                    state = BlockState::Table(vec![line.to_string()]);
                }
            }
            continue;
        }

        // 3. Header check
        if let Some((level, content)) = parse_header(line) {
            emit(&mut state, &mut html);
            html.push_str(&format!(
                "<h{}>{}</h{}>",
                level,
                format_inline(&html_escape(&content)),
                level
            ));
            continue;
        }

        // 4. Unordered list item check
        if let Some(content) = parse_unordered_list_item(line) {
            match state {
                BlockState::UnorderedList(ref mut items) => {
                    items.push(content);
                }
                _ => {
                    emit(&mut state, &mut html);
                    state = BlockState::UnorderedList(vec![content]);
                }
            }
            continue;
        }

        // 5. Ordered list item check
        if let Some(content) = parse_ordered_list_item(line) {
            match state {
                BlockState::OrderedList(ref mut items) => {
                    items.push(content);
                }
                _ => {
                    emit(&mut state, &mut html);
                    state = BlockState::OrderedList(vec![content]);
                }
            }
            continue;
        }

        // 6. Otherwise, it is a paragraph line
        match state {
            BlockState::Paragraph(ref mut lines) => {
                lines.push(line.to_string());
            }
            _ => {
                emit(&mut state, &mut html);
                state = BlockState::Paragraph(vec![line.to_string()]);
            }
        }
    }

    emit(&mut state, &mut html);
    html
}

fn format_inline(text: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        // 1. Inline code
        if chars[i] == '`' {
            let mut j = i + 1;
            while j < chars.len() && chars[j] != '`' {
                j += 1;
            }
            if j < chars.len() {
                let code_content: String = chars[i + 1..j].iter().collect();
                result.push_str(&format!(
                    "<code class=\"inline-code\">{}</code>",
                    code_content
                ));
                i = j + 1;
                continue;
            }
        }

        // 2. Bold (double asterisk / double underscore)
        if i + 1 < chars.len() && chars[i] == '*' && chars[i + 1] == '*' {
            let mut j = i + 2;
            while j + 1 < chars.len() && !(chars[j] == '*' && chars[j + 1] == '*') {
                j += 1;
            }
            if j + 1 < chars.len() {
                let bold_content: String = chars[i + 2..j].iter().collect();
                result.push_str(&format!(
                    "<strong>{}</strong>",
                    format_inline(&bold_content)
                ));
                i = j + 2;
                continue;
            }
        }
        if i + 1 < chars.len() && chars[i] == '_' && chars[i + 1] == '_' {
            let mut j = i + 2;
            while j + 1 < chars.len() && !(chars[j] == '_' && chars[j + 1] == '_') {
                j += 1;
            }
            if j + 1 < chars.len() {
                let bold_content: String = chars[i + 2..j].iter().collect();
                result.push_str(&format!(
                    "<strong>{}</strong>",
                    format_inline(&bold_content)
                ));
                i = j + 2;
                continue;
            }
        }

        // 3. Links [text](url)
        if chars[i] == '[' {
            let mut j = i + 1;
            while j < chars.len() && chars[j] != ']' {
                j += 1;
            }
            if j < chars.len() && j + 1 < chars.len() && chars[j + 1] == '(' {
                let mut k = j + 2;
                while k < chars.len() && chars[k] != ')' {
                    k += 1;
                }
                if k < chars.len() {
                    let text_content: String = chars[i + 1..j].iter().collect();
                    let url_content: String = chars[j + 2..k].iter().collect();
                    result.push_str(&format!(
                        "<a href=\"{}\" target=\"_blank\" rel=\"noopener noreferrer\">{}</a>",
                        url_content,
                        format_inline(&text_content)
                    ));
                    i = k + 1;
                    continue;
                }
            }
        }

        // 4. Italic (single asterisk / single underscore)
        if chars[i] == '*' {
            let mut j = i + 1;
            while j < chars.len() && chars[j] != '*' {
                j += 1;
            }
            if j < chars.len() {
                let italic_content: String = chars[i + 1..j].iter().collect();
                result.push_str(&format!("<em>{}</em>", format_inline(&italic_content)));
                i = j + 1;
                continue;
            }
        }
        if chars[i] == '_' {
            let mut j = i + 1;
            while j < chars.len() && chars[j] != '_' {
                j += 1;
            }
            if j < chars.len() {
                let italic_content: String = chars[i + 1..j].iter().collect();
                result.push_str(&format!("<em>{}</em>", format_inline(&italic_content)));
                i = j + 1;
                continue;
            }
        }

        // Default: normal char
        result.push(chars[i]);
        i += 1;
    }

    result
}

fn render_table(rows: Vec<&str>) -> String {
    if rows.len() < 2 {
        return rows
            .iter()
            .map(|r| format_inline(&html_escape(r)))
            .collect::<Vec<_>>()
            .join("\n");
    }

    let mut html = String::from("<table><thead><tr>");
    let headers: Vec<&str> = rows[0]
        .split('|')
        .filter(|s| !s.is_empty())
        .map(|s| s.trim())
        .collect();

    for h in headers {
        html.push_str(&format!("<th>{}</th>", format_inline(&html_escape(h))));
    }
    html.push_str("</tr></thead><tbody>");

    for row in rows.iter().skip(1) {
        let line = row.trim();
        if line.starts_with("|---") || line.starts_with("| :---") {
            continue;
        }

        html.push_str("<tr>");
        let cols: Vec<&str> = row
            .split('|')
            .filter(|s| !s.is_empty())
            .map(|s| s.trim())
            .collect();
        for c in cols {
            html.push_str(&format!("<td>{}</td>", format_inline(&html_escape(c))));
        }
        html.push_str("</tr>");
    }
    html.push_str("</tbody></table>");
    html
}

fn format_code_part(part: &str) -> String {
    let lines: Vec<&str> = part.split('\n').collect();
    let mut lang = "code";
    let mut code = part;

    if !lines.is_empty() && !lines[0].trim().is_empty() && !lines[0].contains(' ') {
        lang = lines[0].trim();
        code = part[lines[0].len()..].trim_start();
    }

    div()
        .class("code-block")
        .child(
            div()
                .class("code-header")
                .child(span().class("code-lang").text(lang))
                .child(
                    button()
                        .class("copy-btn")
                        .attr("onclick", "const btn = this; const t = (k, f) => (window.qT ? window.qT(k, f) : f); const code = btn.closest('.code-block').querySelector('code').textContent; navigator.clipboard.writeText(code).then(() => { const orig = btn.innerHTML; btn.innerHTML = '<i class=&quot;fas fa-check&quot;></i> ' + t('ui_code_copied', 'Copied!'); btn.classList.add('success'); setTimeout(() => { btn.innerHTML = orig; btn.classList.remove('success'); }, 2000); }).catch(err => { console.error(err); btn.textContent = t('ui_code_copy_error', 'Error'); setTimeout(() => btn.textContent = t('ui_code_copy', 'Copy'), 2000); });")
                        .child(i().class("far fa-copy"))
                        .child(span().attr("data-i18n", "ui_code_copy").text("Copy"))
                )
        )
        .child(
            pre().child(
                element("code")
                    .text(code.trim())
            )
        )
        .render()
}

fn html_escape(s: &str) -> String {
    s.replace("&", "&amp;")
        .replace("<", "&lt;")
        .replace(">", "&gt;")
        .replace("\"", "&quot;")
        .replace("'", "&#39;")
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
