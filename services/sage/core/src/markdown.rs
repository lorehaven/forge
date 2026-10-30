//! The markdown subset an answer is written in, read into blocks the client draws as elements. Nothing here produces markup: text
//! stays text, so a model (or a web page it quoted) cannot put HTML or script in the page.
//!
//! The subset is the one the service used to render on the server: code fences with a language, headings, bullet and numbered
//! lists, `|` tables, rules, paragraphs, and inline code, bold, italic and links. Answers that ran a tool carry the tool's result
//! as a `<div class="tool-result ...">` card in the stored text; those become [`Block::Tool`].

/// A run of text inside a block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inline {
    Text(String),
    Code(String),
    Strong(Vec<Self>),
    Em(Vec<Self>),
    /// `href` is only ever an http(s) or mailto address; any other link stays plain text.
    Link {
        href: String,
        text: Vec<Self>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    Paragraph(Vec<Inline>),
    Heading(u8, Vec<Inline>),
    List {
        ordered: bool,
        items: Vec<Vec<Inline>>,
    },
    Table {
        head: Vec<Vec<Inline>>,
        rows: Vec<Vec<Vec<Inline>>>,
    },
    Rule,
    /// A fenced block; `lang` is `code` when the fence named none.
    Code {
        lang: String,
        code: String,
    },
    /// What a tool returned: its name, whether it failed, and the text in paragraphs.
    Tool {
        name: String,
        error: bool,
        paragraphs: Vec<String>,
    },
}

/// Reads a whole answer (or one still arriving: an open fence is a code block so far).
#[must_use]
pub fn parse(text: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    for (index, part) in text.split("```").enumerate() {
        if index % 2 == 0 {
            text_part(part, &mut blocks);
        } else {
            blocks.push(code_part(part));
        }
    }
    blocks
}

fn code_part(part: &str) -> Block {
    let first = part.split('\n').next().unwrap_or("");
    if !first.trim().is_empty() && !first.contains(' ') && part.contains('\n') {
        Block::Code {
            lang: first.trim().to_string(),
            code: part[first.len()..].trim().to_string(),
        }
    } else {
        Block::Code {
            lang: "code".to_string(),
            code: part.trim().to_string(),
        }
    }
}

const TOOL_OPEN: &str = "<div class=\"tool-result";

/// Text between fences: tool cards come out whole, the rest is read line by line.
fn text_part(text: &str, blocks: &mut Vec<Block>) {
    let mut rest = text;
    while let Some(start) = rest.find(TOOL_OPEN) {
        lines(&rest[..start], blocks);
        match card_end(&rest[start..]) {
            Some(len) => {
                blocks.push(tool_card(&rest[start..start + len]));
                rest = &rest[start + len..];
            }
            None => {
                // A card that never closed (still arriving): show what there is as text.
                lines(&rest[start..], blocks);
                return;
            }
        }
    }
    lines(rest, blocks);
}

/// The length of the `<div>` that starts `card`, counting nested divs.
fn card_end(card: &str) -> Option<usize> {
    let mut depth = 0_i32;
    let mut at = 0;
    while at < card.len() {
        let open = card[at..].find("<div");
        let close = card[at..].find("</div>");
        match (open, close) {
            (Some(o), Some(c)) if o < c => {
                depth += 1;
                at += o + 4;
            }
            (_, Some(c)) => {
                depth -= 1;
                at += c + 6;
                if depth == 0 {
                    return Some(at);
                }
            }
            (Some(o), None) => {
                depth += 1;
                at += o + 4;
            }
            (None, None) => return None,
        }
    }
    None
}

fn tool_card(card: &str) -> Block {
    let head_end = card.find('>').unwrap_or(0);
    let error = card[..head_end].contains("tool-error");
    let name = between(card, "<span class=\"tool-name\">", "</span>")
        .map(|n| plain(n).trim().to_string())
        .unwrap_or_default();
    let body = card
        .find("<div class=\"tool-content\">")
        .map_or("", |at| &card[at + "<div class=\"tool-content\">".len()..]);
    let text = plain(body);
    let paragraphs = text
        .split("\n\n")
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect();
    Block::Tool {
        name,
        error,
        paragraphs,
    }
}

fn between<'a>(text: &'a str, open: &str, close: &str) -> Option<&'a str> {
    let start = text.find(open)? + open.len();
    let len = text[start..].find(close)?;
    Some(&text[start..start + len])
}

/// Markup removed, block ends kept as line breaks, entities decoded.
fn plain(html: &str) -> String {
    let mut out = String::new();
    let mut rest = html;
    while let Some(lt) = rest.find('<') {
        out.push_str(&rest[..lt]);
        let Some(gt) = rest[lt..].find('>') else {
            rest = "";
            break;
        };
        let tag = rest[lt + 1..lt + gt].trim().to_ascii_lowercase();
        let name: String = tag
            .trim_start_matches('/')
            .chars()
            .take_while(char::is_ascii_alphanumeric)
            .collect();
        match (name.as_str(), tag.starts_with('/')) {
            ("li", false) => out.push_str("\n- "),
            ("p" | "h1" | "h2" | "h3" | "h4" | "pre" | "ul" | "ol" | "div", true) => {
                out.push_str("\n\n");
            }
            ("br", _) => out.push('\n'),
            _ => {}
        }
        rest = &rest[lt + gt + 1..];
    }
    out.push_str(rest);
    out.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

enum Open {
    None,
    Paragraph(Vec<String>),
    Table(Vec<String>),
    List { ordered: bool, items: Vec<String> },
}

fn close(open: &mut Open, blocks: &mut Vec<Block>) {
    match std::mem::replace(open, Open::None) {
        Open::None => {}
        Open::Paragraph(lines) => {
            if !lines.is_empty() {
                blocks.push(Block::Paragraph(inline(&lines.join(" "))));
            }
        }
        Open::Table(rows) => blocks.push(table(&rows)),
        Open::List { ordered, items } => blocks.push(Block::List {
            ordered,
            items: items.iter().map(|i| inline(i)).collect(),
        }),
    }
}

fn lines(text: &str, blocks: &mut Vec<Block>) {
    let mut open = Open::None;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            // A blank line ends a paragraph or a table; a list goes on across it.
            if matches!(open, Open::Paragraph(_) | Open::Table(_)) {
                close(&mut open, blocks);
            }
            continue;
        }
        if is_rule(trimmed) {
            close(&mut open, blocks);
            blocks.push(Block::Rule);
        } else if trimmed.starts_with('|') && trimmed.ends_with('|') && trimmed.len() > 1 {
            if let Open::Table(rows) = &mut open {
                rows.push(line.to_string());
            } else {
                close(&mut open, blocks);
                open = Open::Table(vec![line.to_string()]);
            }
        } else if let Some((level, content)) = heading(trimmed) {
            close(&mut open, blocks);
            blocks.push(Block::Heading(level, inline(&content)));
        } else if let Some(item) = bullet(trimmed) {
            list_item(&mut open, blocks, false, item);
        } else if let Some(item) = numbered(trimmed) {
            list_item(&mut open, blocks, true, item);
        } else if let Open::Paragraph(lines) = &mut open {
            lines.push(trimmed.to_string());
        } else {
            close(&mut open, blocks);
            open = Open::Paragraph(vec![trimmed.to_string()]);
        }
    }
    close(&mut open, blocks);
}

fn list_item(open: &mut Open, blocks: &mut Vec<Block>, ordered: bool, item: String) {
    if let Open::List {
        ordered: same,
        items,
    } = open
        && *same == ordered
    {
        items.push(item);
        return;
    }
    close(open, blocks);
    *open = Open::List {
        ordered,
        items: vec![item],
    };
}

fn is_rule(line: &str) -> bool {
    let Some(first) = line.chars().next() else {
        return false;
    };
    line.chars().count() >= 3
        && matches!(first, '-' | '*' | '_')
        && line.chars().all(|c| c == first)
}

fn heading(line: &str) -> Option<(u8, String)> {
    let level = line.chars().take_while(|c| *c == '#').count();
    if (1..=6).contains(&level) && line[level..].starts_with(' ') {
        Some((u8::try_from(level).ok()?, line[level..].trim().to_string()))
    } else {
        None
    }
}

fn bullet(line: &str) -> Option<String> {
    ["- ", "* ", "+ "]
        .iter()
        .find_map(|p| line.strip_prefix(p))
        .map(|rest| rest.trim().to_string())
}

fn numbered(line: &str) -> Option<String> {
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    if digits == 0 {
        return None;
    }
    line[digits..]
        .strip_prefix(". ")
        .map(|rest| rest.trim().to_string())
}

fn cells(row: &str) -> Vec<Vec<Inline>> {
    row.split('|')
        .filter(|c| !c.is_empty())
        .map(|c| inline(c.trim()))
        .collect()
}

fn table(rows: &[String]) -> Block {
    if rows.len() < 2 {
        return Block::Paragraph(inline(&rows.join(" ")));
    }
    let body = rows[1..]
        .iter()
        .filter(|r| {
            let r = r.trim();
            !(r.starts_with("|---") || r.starts_with("| :---") || r.starts_with("|:---"))
        })
        .map(|r| cells(r))
        .collect();
    Block::Table {
        head: cells(&rows[0]),
        rows: body,
    }
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric()
}

fn safe_href(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    lower.starts_with("https://") || lower.starts_with("http://") || lower.starts_with("mailto:")
}

fn push_text(out: &mut Vec<Inline>, text: &str) {
    if text.is_empty() {
        return;
    }
    if let Some(Inline::Text(last)) = out.last_mut() {
        last.push_str(text);
    } else {
        out.push(Inline::Text(text.to_string()));
    }
}

/// The index of `needle` in `chars` at or after `from`.
fn find(chars: &[char], from: usize, needle: &[char]) -> Option<usize> {
    if needle.is_empty() || chars.len() < needle.len() {
        return None;
    }
    (from..=chars.len() - needle.len()).find(|&i| chars[i..i + needle.len()] == *needle)
}

/// Bold, italic, inline code and links.
#[must_use]
pub fn inline(text: &str) -> Vec<Inline> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut plain = String::new();
    let mut i = 0;
    let flush = |out: &mut Vec<Inline>, plain: &mut String| {
        push_text(out, plain);
        plain.clear();
    };
    while i < chars.len() {
        let c = chars[i];
        let before = if i == 0 { None } else { Some(chars[i - 1]) };
        // Inline code.
        if c == '`'
            && let Some(end) = find(&chars, i + 1, &['`'])
            && end > i + 1
        {
            flush(&mut out, &mut plain);
            out.push(Inline::Code(chars[i + 1..end].iter().collect()));
            i = end + 1;
            continue;
        }
        // Bold: `**x**`, and `__x__` at a word edge.
        if i + 1 < chars.len() && (c == '*' || c == '_') && chars[i + 1] == c {
            let pair = [c, c];
            let edge = c == '*' || !before.is_some_and(is_word);
            if edge
                && !chars[i + 2..].first().is_some_and(|c| c.is_whitespace())
                && let Some(end) = find(&chars, i + 2, &pair)
                && end > i + 2
                && !chars[end - 1].is_whitespace()
                && (c == '*' || !chars.get(end + 2).copied().is_some_and(is_word))
            {
                flush(&mut out, &mut plain);
                out.push(Inline::Strong(inline(
                    &chars[i + 2..end].iter().collect::<String>(),
                )));
                i = end + 2;
                continue;
            }
        }
        // Link: `[text](url)`.
        if c == '['
            && let Some(close) = find(&chars, i + 1, &[']'])
            && chars.get(close + 1) == Some(&'(')
            && let Some(end) = find(&chars, close + 2, &[')'])
        {
            let label: String = chars[i + 1..close].iter().collect();
            let href: String = chars[close + 2..end].iter().collect();
            flush(&mut out, &mut plain);
            if safe_href(&href) {
                out.push(Inline::Link {
                    href: href.trim().to_string(),
                    text: inline(&label),
                });
            } else {
                push_text(&mut out, &label);
            }
            i = end + 1;
            continue;
        }
        // Italic: `*x*`, and `_x_` at a word edge (so `snake_case` stays as it is).
        if c == '*' || c == '_' {
            let edge = c == '*' || !before.is_some_and(is_word);
            if edge
                && !chars.get(i + 1).is_some_and(|c| c.is_whitespace())
                && let Some(end) = find(&chars, i + 1, &[c])
                && end > i + 1
                && !chars[end - 1].is_whitespace()
                && (c == '*' || !chars.get(end + 1).copied().is_some_and(is_word))
            {
                flush(&mut out, &mut plain);
                out.push(Inline::Em(inline(
                    &chars[i + 1..end].iter().collect::<String>(),
                )));
                i = end + 1;
                continue;
            }
        }
        plain.push(c);
        i += 1;
    }
    flush(&mut out, &mut plain);
    out
}

/// The text of inline runs without any markup: a tooltip, a copy.
#[must_use]
pub fn plain_text(runs: &[Inline]) -> String {
    let mut out = String::new();
    for run in runs {
        match run {
            Inline::Text(t) | Inline::Code(t) => out.push_str(t),
            Inline::Strong(inner) | Inline::Em(inner) | Inline::Link { text: inner, .. } => {
                out.push_str(&plain_text(inner));
            }
        }
    }
    out
}
