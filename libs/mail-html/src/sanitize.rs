//! Making a message's HTML safe to show, and turning it into text.
//!
//! Mail HTML is written by strangers. [`sanitize`] reads it with a tolerant parser and writes a **new** document from an
//! allow-list: only known tags, only known attributes, only a safe subset of inline CSS, links only to `http`, `https`,
//! `mailto` and `tel`, and images only from the message itself (`cid:`, `data:`) unless the reader asked for remote ones.
//! Nothing is "removed from" the input: what is not on the list is never written. Whatever is left is still shown inside a
//! sandboxed frame with a restrictive Content-Security-Policy (see the service); this is the first of those two layers.

use quench_html::dom::{Document, Node, NodeKind};

/// What the caller wants from [`sanitize`].
#[derive(Debug, Clone)]
pub struct Options {
    /// Keep `http(s)` images (the reader allowed them for this message); otherwise they are counted and left out.
    pub allow_remote_images: bool,
    /// Most bytes of HTML written; the rest of the document is dropped.
    pub max_output: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            allow_remote_images: false,
            max_output: 2 * 1024 * 1024,
        }
    }
}

/// The sanitised document and what was held back.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Sanitized {
    /// A fragment: the content of `<body>`, safe markup only.
    pub html: String,
    /// Remote images left out (a tracking pixel counts).
    pub remote_blocked: usize,
    /// The output hit `max_output` and was cut.
    pub truncated: bool,
}

/// Deepest element nesting written as markup; deeper content is flattened to its text.
const MAX_DEPTH: usize = 64;
/// Largest `data:` image kept (bytes of the URI).
const MAX_DATA_URI: usize = 1024 * 1024;

const ALLOWED_TAGS: &[&str] = &[
    "a",
    "abbr",
    "address",
    "b",
    "bdi",
    "bdo",
    "big",
    "blockquote",
    "br",
    "caption",
    "center",
    "cite",
    "code",
    "col",
    "colgroup",
    "dd",
    "del",
    "dfn",
    "div",
    "dl",
    "dt",
    "em",
    "font",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "hr",
    "i",
    "img",
    "ins",
    "kbd",
    "li",
    "mark",
    "ol",
    "p",
    "pre",
    "q",
    "s",
    "samp",
    "small",
    "span",
    "strike",
    "strong",
    "sub",
    "sup",
    "table",
    "tbody",
    "td",
    "tfoot",
    "th",
    "thead",
    "tr",
    "tt",
    "u",
    "ul",
    "var",
    "wbr",
];

/// Elements whose content must not appear at all.
const DROPPED_WITH_CONTENT: &[&str] = &[
    "script",
    "style",
    "head",
    "title",
    "iframe",
    "frame",
    "frameset",
    "object",
    "embed",
    "applet",
    "noscript",
    "template",
    "svg",
    "math",
    "input",
    "button",
    "select",
    "textarea",
    "option",
    "audio",
    "video",
    "source",
    "track",
    "canvas",
    "map",
    "area",
    "meta",
    "link",
    "base",
    "param",
    "dialog",
    "xmp",
    "plaintext",
    "noembed",
    "noframes",
];

/// The sanitised form of `html`. `resolve_cid` maps a `cid:` reference (without the scheme) to the URL the page can load
/// that part from, or `None` when the message has no such part.
pub fn sanitize(
    html: &str,
    options: &Options,
    resolve_cid: impl Fn(&str) -> Option<String>,
) -> Sanitized {
    let document = Document::parse(html);
    let mut writer = Writer {
        out: String::new(),
        options,
        resolve_cid: &resolve_cid,
        remote_blocked: 0,
        truncated: false,
    };
    if let Some(body) = body_of(&document) {
        for child in body.children() {
            writer.node(child, 0);
        }
    }
    Sanitized {
        html: writer.out,
        remote_blocked: writer.remote_blocked,
        truncated: writer.truncated,
    }
}

fn body_of(document: &Document) -> Option<Node<'_>> {
    document
        .root_element()
        .children()
        .find(|n| n.name() == Some("body"))
}

struct Writer<'a> {
    out: String,
    options: &'a Options,
    resolve_cid: &'a dyn Fn(&str) -> Option<String>,
    remote_blocked: usize,
    truncated: bool,
}

impl Writer<'_> {
    fn full(&mut self) -> bool {
        if self.out.len() >= self.options.max_output {
            self.truncated = true;
        }
        self.truncated
    }

    fn node(&mut self, node: Node<'_>, depth: usize) {
        if self.full() {
            return;
        }
        match node.kind() {
            NodeKind::Text => {
                if let Some(text) = node.as_text() {
                    escape(text, false, &mut self.out);
                }
            }
            NodeKind::Element => self.element(node, depth),
            NodeKind::Document | NodeKind::Comment | NodeKind::Doctype => {}
        }
    }

    fn element(&mut self, node: Node<'_>, depth: usize) {
        let Some(name) = node.name() else {
            return;
        };
        let name = name.to_ascii_lowercase();
        if DROPPED_WITH_CONTENT.contains(&name.as_str()) {
            return;
        }
        if depth >= MAX_DEPTH {
            // Too deep to trust markup: keep the words only.
            escape(&node.text(), false, &mut self.out);
            return;
        }
        if !ALLOWED_TAGS.contains(&name.as_str()) {
            // Unknown or unwanted wrapper (html, body, form, label, ...): keep what is inside.
            for child in node.children() {
                self.node(child, depth + 1);
            }
            return;
        }
        if name == "img" {
            self.image(node);
            return;
        }
        self.out.push('<');
        self.out.push_str(&name);
        self.attributes(node, &name);
        self.out.push('>');
        if matches!(name.as_str(), "br" | "hr" | "col" | "wbr") {
            return;
        }
        for child in node.children() {
            self.node(child, depth + 1);
        }
        self.out.push_str("</");
        self.out.push_str(&name);
        self.out.push('>');
    }

    fn attributes(&mut self, node: Node<'_>, tag: &str) {
        let mut href_written = false;
        for (name, value) in node.attrs() {
            let name = name.to_ascii_lowercase();
            let value = value.trim();
            let kept: Option<String> = match name.as_str() {
                "href" if tag == "a" => safe_link(value).inspect(|_| href_written = true),
                "title" => Some(strip_controls(value)),
                "lang" if token(value) => Some(value.to_string()),
                "dir" if matches!(value.to_ascii_lowercase().as_str(), "ltr" | "rtl" | "auto") => {
                    Some(value.to_ascii_lowercase())
                }
                "align" | "valign"
                    if matches!(
                        value.to_ascii_lowercase().as_str(),
                        "left" | "right" | "center" | "justify" | "top" | "middle" | "bottom"
                    ) =>
                {
                    Some(value.to_ascii_lowercase())
                }
                "color" | "bgcolor" if color(value) => Some(value.to_string()),
                "width" | "height" | "border" | "cellpadding" | "cellspacing" | "colspan"
                | "rowspan" | "size" | "start"
                    if dimension(value) =>
                {
                    Some(value.to_string())
                }
                "style" => {
                    let filtered = filter_style(value);
                    (!filtered.is_empty()).then_some(filtered)
                }
                _ => None,
            };
            if let Some(kept) = kept {
                self.out.push(' ');
                self.out.push_str(&name);
                self.out.push_str("=\"");
                escape(&kept, true, &mut self.out);
                self.out.push('"');
            }
        }
        if tag == "a" && href_written {
            self.out
                .push_str(" target=\"_blank\" rel=\"noopener noreferrer nofollow\"");
        }
    }

    fn image(&mut self, node: Node<'_>) {
        let source = node.attr("src").map_or("", str::trim);
        let lower = source.to_ascii_lowercase();
        let mut src: Option<String> = None;
        let mut blocked = false;
        if let Some(id) = lower.strip_prefix("cid:") {
            let id = source[4..4 + id.len()].trim().trim_matches(['<', '>']);
            src = (self.resolve_cid)(id);
        } else if let Some(rest) = lower.strip_prefix("data:") {
            let allowed = ["image/png;", "image/jpeg;", "image/gif;", "image/webp;"]
                .iter()
                .any(|prefix| rest.starts_with(prefix))
                && rest.contains(";base64,")
                && source.len() <= MAX_DATA_URI
                && source
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"+/=;:,-".contains(&b));
            if allowed {
                src = Some(source.to_string());
            }
        } else if lower.starts_with("http://") || lower.starts_with("https://") {
            if self.options.allow_remote_images && no_controls(source) {
                src = Some(source.to_string());
            } else {
                blocked = true;
                self.remote_blocked += 1;
            }
        }
        self.out.push_str("<img");
        if let Some(src) = src {
            self.out.push_str(" src=\"");
            escape(&src, true, &mut self.out);
            self.out.push('"');
        } else if blocked {
            self.out.push_str(" data-blocked=\"remote\"");
        }
        for (name, value) in node.attrs() {
            let name = name.to_ascii_lowercase();
            let value = value.trim();
            let kept = match name.as_str() {
                "alt" | "title" => Some(strip_controls(value)),
                "width" | "height" if dimension(value) => Some(value.to_string()),
                "style" => {
                    let filtered = filter_style(value);
                    (!filtered.is_empty()).then_some(filtered)
                }
                _ => None,
            };
            if let Some(kept) = kept {
                self.out.push(' ');
                self.out.push_str(&name);
                self.out.push_str("=\"");
                escape(&kept, true, &mut self.out);
                self.out.push('"');
            }
        }
        self.out.push('>');
    }
}

fn escape(text: &str, attribute: bool, out: &mut String) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' if attribute => out.push_str("&quot;"),
            '\'' if attribute => out.push_str("&#39;"),
            '\0' => {}
            c => out.push(c),
        }
    }
}

fn strip_controls(value: &str) -> String {
    value.chars().filter(|c| !c.is_control()).collect()
}

fn no_controls(value: &str) -> bool {
    !value.chars().any(|c| c.is_control() || c.is_whitespace())
}

fn token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 16
        && value.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

fn dimension(value: &str) -> bool {
    let digits = value
        .strip_suffix('%')
        .or_else(|| value.strip_suffix("px"))
        .unwrap_or(value);
    !digits.is_empty() && digits.len() <= 5 && digits.bytes().all(|b| b.is_ascii_digit())
}

/// A colour: `#rgb`, `#rrggbb`, `#rrggbbaa`, a keyword, or `rgb(...)`/`rgba(...)` with plain numbers.
fn color(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    if let Some(hex) = lower.strip_prefix('#') {
        return matches!(hex.len(), 3 | 4 | 6 | 8) && hex.bytes().all(|b| b.is_ascii_hexdigit());
    }
    if lower.starts_with("rgb(") || lower.starts_with("rgba(") || lower.starts_with("hsl(") {
        return lower.len() <= 40
            && lower
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"(),. %".contains(&b));
    }
    !lower.is_empty() && lower.len() <= 24 && lower.bytes().all(|b| b.is_ascii_alphabetic())
}

/// The URL if it is one a message may link to; `None` otherwise (the link text stays, the link goes).
#[must_use]
pub fn safe_link(value: &str) -> Option<String> {
    // Browsers ignore whitespace and control characters inside a scheme (`java\tscript:`); so refuse any.
    if value.is_empty() || value.len() > 2048 || !no_controls(value) {
        return None;
    }
    let lower = value.to_ascii_lowercase();
    let ok = lower.starts_with("https://")
        || lower.starts_with("http://")
        || lower.starts_with("mailto:")
        || lower.starts_with("tel:");
    ok.then(|| value.to_string())
}

const STYLE_PROPERTIES: &[&str] = &[
    "color",
    "background-color",
    "background",
    "font",
    "font-family",
    "font-size",
    "font-weight",
    "font-style",
    "text-align",
    "text-decoration",
    "text-transform",
    "text-indent",
    "line-height",
    "letter-spacing",
    "white-space",
    "vertical-align",
    "width",
    "height",
    "max-width",
    "min-width",
    "max-height",
    "min-height",
    "margin",
    "margin-top",
    "margin-right",
    "margin-bottom",
    "margin-left",
    "padding",
    "padding-top",
    "padding-right",
    "padding-bottom",
    "padding-left",
    "border",
    "border-top",
    "border-right",
    "border-bottom",
    "border-left",
    "border-color",
    "border-style",
    "border-width",
    "border-radius",
    "border-collapse",
    "border-spacing",
    "list-style-type",
    "display",
    "word-break",
    "overflow-wrap",
];

/// An inline `style` reduced to the properties above, with values that cannot load or run anything.
fn filter_style(style: &str) -> String {
    let mut out = String::new();
    for declaration in style.split(';') {
        let Some((property, value)) = declaration.split_once(':') else {
            continue;
        };
        let property = property.trim().to_ascii_lowercase();
        let value = value.trim();
        let lower = value.to_ascii_lowercase();
        let lower_compact: String = lower.chars().filter(|c| !c.is_whitespace()).collect();
        let dangerous = lower_compact.contains("url(")
            || lower_compact.contains("expression")
            || lower_compact.contains("javascript")
            || lower_compact.contains("behavior")
            || lower_compact.contains("binding")
            || lower_compact.contains("image-set")
            || lower_compact.contains("@import")
            || lower_compact.contains("var(")
            || lower_compact.contains("env(")
            || lower_compact.contains("attr(");
        let clean = value.len() <= 200
            && !dangerous
            && value
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || " #%.,()-_'\"!/".contains(c))
            && !value.contains("/*");
        let display_ok = property != "display"
            || matches!(
                lower.as_str(),
                "none" | "block" | "inline" | "inline-block" | "table" | "table-cell" | "table-row"
            );
        if STYLE_PROPERTIES.contains(&property.as_str()) && clean && display_ok && !value.is_empty()
        {
            if !out.is_empty() {
                out.push(';');
            }
            out.push_str(&property);
            out.push(':');
            out.push_str(value);
        }
    }
    out
}

const BLOCKS: &[&str] = &[
    "p",
    "div",
    "table",
    "tr",
    "ul",
    "ol",
    "li",
    "blockquote",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "hr",
    "pre",
    "dl",
    "dt",
    "dd",
    "address",
    "center",
    "caption",
    "thead",
    "tbody",
    "tfoot",
];

/// The words of an HTML document as plain text: paragraphs and rows on their own lines, list items marked, links followed by
/// their address, scripts and styles left out.
#[must_use]
pub fn to_text(html: &str) -> String {
    let document = Document::parse(html);
    let mut out = String::new();
    if let Some(body) = body_of(&document) {
        for child in body.children() {
            text_of(child, &mut out, false, 0);
        }
    }
    tidy(&out)
}

fn text_of(node: Node<'_>, out: &mut String, preformatted: bool, depth: usize) {
    if out.len() > 8 * 1024 * 1024 {
        return;
    }
    match node.kind() {
        NodeKind::Text => {
            let text = node.as_text().unwrap_or_default();
            if preformatted {
                out.push_str(text);
            } else {
                let mut last_space = out.ends_with([' ', '\n']) || out.is_empty();
                for c in text.chars() {
                    if c.is_whitespace() {
                        if !last_space {
                            out.push(' ');
                            last_space = true;
                        }
                    } else if !c.is_control() {
                        out.push(c);
                        last_space = false;
                    }
                }
            }
        }
        NodeKind::Element => {
            let name = node.name().unwrap_or("").to_ascii_lowercase();
            if DROPPED_WITH_CONTENT.contains(&name.as_str()) && name != "title" {
                return;
            }
            if depth > 200 {
                out.push_str(&node.text());
                return;
            }
            match name.as_str() {
                "br" => {
                    out.push('\n');
                    return;
                }
                "img" => {
                    if let Some(alt) = node.attr("alt").filter(|a| !a.trim().is_empty()) {
                        out.push_str(alt.trim());
                    }
                    return;
                }
                "li" => {
                    if !out.ends_with('\n') {
                        out.push('\n');
                    }
                    out.push_str("- ");
                }
                "td" | "th" if !out.ends_with(['\n', ' ']) => out.push(' '),
                n if BLOCKS.contains(&n) && !out.ends_with('\n') && !out.is_empty() => {
                    out.push('\n');
                }
                _ => {}
            }
            let pre = preformatted || name == "pre";
            for child in node.children() {
                text_of(child, out, pre, depth + 1);
            }
            if name == "a"
                && let Some(href) = node.attr("href")
                && let Some(link) = safe_link(href.trim())
                && link.starts_with("http")
                && node.text().trim() != link
            {
                out.push_str(" (");
                out.push_str(&link);
                out.push(')');
            }
            if BLOCKS.contains(&name.as_str()) && !out.ends_with('\n') {
                out.push('\n');
            }
        }
        _ => {}
    }
}

fn tidy(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut blank = 0;
    for line in text.lines() {
        let line = line.trim_end();
        if line.trim().is_empty() {
            blank += 1;
            if blank > 1 {
                continue;
            }
        } else {
            blank = 0;
        }
        out.push_str(line);
        out.push('\n');
    }
    out.trim().to_string()
}
