use regex::Regex;
use std::sync::LazyLock;

static INLINE_CODE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"`([^`]+)`").unwrap());

pub fn format_message(text: &str) -> String {
    let parts: Vec<&str> = text.split("```").collect();
    let mut html = String::new();

    for (i, part) in parts.iter().enumerate() {
        if i % 2 == 0 {
            // Text part (might contain tables)
            html.push_str(&format_text_part(part));
        } else {
            // Code part
            html.push_str(&format_code_part(part));
        }
    }
    html
}

fn format_text_part(text: &str) -> String {
    let mut result = Vec::new();
    let mut table_rows = Vec::new();
    let mut in_table = false;

    let lines: Vec<&str> = text.split('\n').collect();

    for line in lines {
        let trimmed = line.trim();
        if trimmed.starts_with('|') && trimmed.ends_with('|') {
            in_table = true;
            table_rows.push(line);
        } else {
            if in_table {
                result.push(render_table(table_rows.clone()));
                table_rows.clear();
                in_table = false;
            }
            result.push(format_inline(&html_escape(line)));
        }
    }

    if in_table {
        result.push(render_table(table_rows));
    }

    result.join("\n")
}

fn format_inline(text: &str) -> String {
    let mut result = String::new();
    let mut last_pos = 0;
    for cap in INLINE_CODE_RE.captures_iter(text) {
        let m = cap.get(0).unwrap();
        result.push_str(&text[last_pos..m.start()]);
        result.push_str(&format!("<code class=\"inline-code\">{}</code>", &cap[1]));
        last_pos = m.end();
    }
    result.push_str(&text[last_pos..]);
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

    format!(
        r#"<div class="code-block">
                <div class="code-header">
                    <span class="code-lang">{}</span>
                    <button class="copy-btn">
                        <i class="far fa-copy"></i>
                        <span>Copy</span>
                    </button>
                </div>
                <pre><code>{}</code></pre>
             </div>"#,
        html_escape(lang),
        html_escape(code.trim())
    )
}

fn html_escape(s: &str) -> String {
    s.replace("&", "&amp;")
        .replace("<", "&lt;")
        .replace(">", "&gt;")
        .replace("\"", "&quot;")
        .replace("'", "&#39;")
}
