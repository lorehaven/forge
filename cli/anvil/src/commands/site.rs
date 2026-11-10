//! The site of a book: its Markdown pages as HTML pages with the navigation beside them, a search over them, and the files
//! that go with them (images, downloads) - what `anvil docs build` used Jekyll for.
//!
//! The pages and the navigation are `docs-book`'s: the same tree `anvil docs check` checks and loupe browses. This turns each
//! page into HTML (`CommonMark` with the GFM extensions, from `quench-html`), gives its headings anchors, points the links
//! between pages at the pages written here, and puts it in a layout of a sidebar (the navigation), a header (search, theme)
//! and the page with its neighbours. Links are written from the site's root (`/x/`, with the book's `baseurl` in front when it
//! has one), so a server that hosts the book under a prefix can rewrite them, as Lectern does.
//!
//! Not done, as Jekyll did it: syntax highlighting of code blocks (they are plain `<pre><code class="language-x">`), Liquid
//! beyond what the books use (`{% raw %}`, `{{ site.baseurl }}`, `{{ site.title }}`, `{{ page.title }}`), kramdown's attribute
//! lists, collections and plugins.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::Path;

use docs_book::{Book, Nav, Page, Target, is_outside, split_front_matter};
use quench_core::{Context, Result, bail};
use quench_html::markdown::{self, Options};
use quench_serde::Value;

const LAYOUT_CSS: &str = include_str!("../../assets/docs-theme/layout.css");
const TOKENS_CSS: &str = include_str!("../../assets/docs-theme/forge-tokens.css");
const FORGE_CSS: &str = include_str!("../../assets/docs-theme/forge.css");
const SITE_JS: &str = include_str!("../../assets/docs-theme/site.js");
const HEAD: &str = include_str!("../../assets/docs-theme/head_custom.html");
const HEADER: &str = include_str!("../../assets/docs-theme/header_custom.html");
const TITLE: &str = include_str!("../../assets/docs-theme/title.html");

/// How the site is made.
#[derive(Debug, Clone, Default)]
pub struct SiteOptions {
    /// The path the site is served under (`/docs`), or empty.
    pub baseurl: String,
    /// Without the forge theme the site has the layout and nothing else of a look: no colours, no theme menu.
    pub plain: bool,
}

/// What was written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Written {
    pub pages: usize,
    pub files: usize,
}

// ---------------------------------------------------------------- where a page is

/// Where a page is written (`services/gauge/index.html`) and its address below the site's root (`services/gauge/`).
fn location(page: &Page) -> (String, String) {
    if let Some(permalink) = &page.permalink {
        let rooted = permalink.trim_start_matches('/');
        if rooted.is_empty() {
            return ("index.html".to_string(), String::new());
        }
        if rooted.ends_with('/') {
            return (format!("{rooted}index.html"), rooted.to_string());
        }
        if rooted
            .rsplit('/')
            .next()
            .is_some_and(|name| name.contains('.'))
        {
            return (rooted.to_string(), rooted.to_string());
        }
        return (format!("{rooted}.html"), format!("{rooted}.html"));
    }
    let route = page.route();
    if page.path == "index.md" || page.path.ends_with("/index.md") {
        if route.is_empty() {
            ("index.html".to_string(), String::new())
        } else {
            (format!("{route}/index.html"), format!("{route}/"))
        }
    } else {
        (format!("{route}.html"), format!("{route}.html"))
    }
}

struct Site<'a> {
    book: &'a Book,
    title: String,
    baseurl: String,
    plain: bool,
    /// `(file, address)` of each page.
    places: Vec<(String, String)>,
    liquid: Vec<bool>,
}

impl Site<'_> {
    /// The address of an address below the root, with the base in front.
    fn url(&self, rel: &str) -> String {
        format!("{}/{rel}", self.baseurl)
    }

    fn page_url(&self, page: usize) -> String {
        self.url(&self.places[page].1)
    }
}

// ---------------------------------------------------------------- Liquid, as far as the books use it

/// What a book's `_config.yml` says about Liquid: the `render_with_liquid` of the `defaults` whose scope covers `path`, the
/// last one winning, as in Jekyll.
fn config_liquid(config: Option<&Value>, path: &str) -> Option<bool> {
    let defaults = config?.get("defaults")?.as_array()?;
    let mut found = None;
    for rule in defaults {
        let scope = rule
            .get("scope")
            .and_then(|s| s.get("path"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        let covers = scope.is_empty() || path == scope || path.starts_with(&format!("{scope}/"));
        if covers
            && let Some(on) = rule
                .get("values")
                .and_then(|v| v.get("render_with_liquid"))
                .and_then(Value::as_bool)
        {
            found = Some(on);
        }
    }
    found
}

/// Whether Liquid is on for the page at `path` whose text is `text`.
fn liquid_on(config: Option<&Value>, path: &str, text: &str) -> bool {
    let own = split_front_matter(text)
        .and_then(|(head, _)| quench_serde::yaml::from_str::<Value>(head).ok())
        .and_then(|v| v.get("render_with_liquid").and_then(Value::as_bool));
    own.or_else(|| config_liquid(config, path)).unwrap_or(true)
}

/// The Liquid the books use, evaluated: `{% raw %}` and `{% endraw %}` are dropped and the three variables replaced.
fn liquid(text: &str, site: &Site<'_>, title: &str) -> String {
    let mut out = text.to_string();
    for (variable, value) in [
        ("site.baseurl", site.baseurl.as_str()),
        ("site.title", site.title.as_str()),
        ("page.title", title),
    ] {
        for form in [
            format!("{{{{ {variable} }}}}"),
            format!("{{{{{variable}}}}}"),
        ] {
            out = out.replace(&form, value);
        }
    }
    out = out.replace("{% raw %}", "").replace("{% endraw %}", "");
    out
}

// ---------------------------------------------------------------- HTML

#[must_use]
fn esc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            other => out.push(other),
        }
    }
    out
}

/// The text of some HTML: its tags removed and its entities read. A tag is a break between words when `apart`, as between the
/// blocks of a page, and nothing at all when not, as inside a heading (`The <code>x</code>` is `The x`).
fn text_in(html: &str, apart: bool) -> String {
    let mut out = String::with_capacity(html.len());
    let mut inside = false;
    for c in html.chars() {
        match c {
            '<' => inside = true,
            '>' if inside => {
                inside = false;
                if apart {
                    out.push(' ');
                }
            }
            _ if !inside => out.push(c),
            _ => {}
        }
    }
    let decoded = quench_html::decode_entities(&out);
    decoded.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn text_of(html: &str) -> String {
    text_in(html, true)
}

/// A heading's anchor, the way GitHub-flavoured kramdown writes it: lower case, letters, digits, `_` and `-` kept, spaces
/// becoming `-`, everything else gone, and no tidying of what is left (`loupe --book` is `loupe---book`).
fn slug(text: &str) -> String {
    let mut out = String::new();
    for c in text.trim().chars() {
        if c.is_alphanumeric() || c == '_' || c == '-' {
            out.extend(c.to_lowercase());
        } else if c == ' ' {
            out.push('-');
        }
    }
    if out.is_empty() {
        "section".to_string()
    } else {
        out
    }
}

/// A heading found in the HTML of a page.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Heading {
    level: usize,
    id: String,
    text: String,
}

/// The page's HTML with an `id` and an anchor link on each heading, the tables in a wrapper that scrolls and the code blocks
/// in the elements the theme styles; and the headings found.
fn decorate(html: &str) -> (String, Vec<Heading>) {
    let mut out = String::with_capacity(html.len() + 512);
    let mut headings = Vec::new();
    let mut used: BTreeSet<String> = BTreeSet::new();
    let mut rest = html;
    while !rest.is_empty() {
        let Some(at) = rest.find('<') else {
            out.push_str(rest);
            break;
        };
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        // <h1>..</h1> without attributes, which is how the renderer writes them.
        if let Some(level) = heading_start(rest) {
            let close = format!("</h{level}>");
            if let Some(end) = rest.find(&close) {
                let (inner, explicit) = explicit_id(&rest[4..end]);
                let text = text_in(inner, false);
                let base = explicit.map_or_else(|| slug(&text), str::to_string);
                let mut id = base.clone();
                let mut n = 1;
                while !used.insert(id.clone()) {
                    id = format!("{base}-{n}");
                    n += 1;
                }
                let _ = write!(
                    out,
                    "<h{level} id=\"{id}\">{inner}<a href=\"#{id}\" class=\"anchor-heading\" aria-label=\"Link to this heading\">#</a>{close}"
                );
                headings.push(Heading { level, id, text });
                rest = &rest[end + close.len()..];
                continue;
            }
        }
        if rest.starts_with("<table") {
            out.push_str("<div class=\"table-wrapper\">");
        }
        if let Some(after) = rest.strip_prefix("</table>") {
            out.push_str("</table></div>");
            rest = after;
            continue;
        }
        if let Some(after) = rest.strip_prefix("<pre><code")
            && let Some(end) = after.find("</code></pre>")
        {
            let open_end = after.find('>').unwrap_or(0);
            let attributes = &after[..open_end];
            let language = attributes
                .split("language-")
                .nth(1)
                .and_then(|l| l.split(['"', ' ']).next())
                .unwrap_or("");
            let class = if language.is_empty() {
                "highlighter-rouge".to_string()
            } else {
                format!("language-{language} highlighter-rouge")
            };
            let code = &after[open_end + 1..end];
            let _ = write!(
                out,
                "<div class=\"{class}\"><div class=\"highlight\"><pre class=\"highlight\"><code>{code}</code></pre></div></div>"
            );
            rest = &after[end + "</code></pre>".len()..];
            continue;
        }
        out.push('<');
        rest = &rest[1..];
    }
    (out, headings)
}

/// A heading written `Title {#id}` (kramdown's explicit id): the title without it, and the id.
fn explicit_id(inner: &str) -> (&str, Option<&str>) {
    let trimmed = inner.trim_end();
    if let Some(open) = trimmed.rfind("{#")
        && let Some(id) = trimmed[open + 2..].strip_suffix('}')
        && !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | ':' | '.'))
    {
        return (trimmed[..open].trim_end(), Some(id));
    }
    (inner, None)
}

/// `Some(n)` when `html` starts with `<hN>`.
fn heading_start(html: &str) -> Option<usize> {
    let bytes = html.as_bytes();
    (bytes.len() > 3
        && bytes[0] == b'<'
        && bytes[1] == b'h'
        && (b'1'..=b'6').contains(&bytes[2])
        && bytes[3] == b'>')
        .then(|| usize::from(bytes[2] - b'0'))
}

/// The page's HTML with the links to other pages of the book pointing at where those pages are written.
fn link_pages(html: &str, site: &Site<'_>, from: usize) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(at) = rest.find("href=\"") {
        let start = at + "href=\"".len();
        out.push_str(&rest[..start]);
        rest = &rest[start..];
        let Some(end) = rest.find('"') else { break };
        let target = quench_html::decode_entities(&rest[..end]);
        let rewritten = if is_outside(&target) {
            None
        } else {
            match site.book.resolve(from, &target) {
                Target::Page { page, anchor } => {
                    let mut url = site.page_url(page);
                    if let Some(anchor) = anchor.filter(|a| !a.is_empty()) {
                        url.push('#');
                        url.push_str(&anchor);
                    }
                    Some(url)
                }
                _ => None,
            }
        };
        match rewritten {
            Some(url) => out.push_str(&esc(&url)),
            None => out.push_str(&rest[..end]),
        }
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

// ---------------------------------------------------------------- the layout

fn nav_list(
    site: &Site<'_>,
    nav: &Nav,
    items: &[usize],
    current: usize,
    trail: &[usize],
    out: &mut String,
) {
    out.push_str("<ul class=\"nav-list\">");
    for &item in items {
        let children = nav.children(item);
        let open = trail.contains(&item) && !children.is_empty();
        let here = item == current;
        let _ = write!(
            out,
            "<li class=\"nav-list-item{}\">",
            if open { " active" } else { "" }
        );
        if !children.is_empty() {
            let title = site.book.pages()[item].label();
            let _ = write!(
                out,
                "<button class=\"nav-list-expander\" aria-label=\"Toggle {} \" aria-pressed=\"{open}\"><svg viewBox=\"0 0 24 24\" aria-hidden=\"true\"><path d=\"M8.6 5.6 15 12l-6.4 6.4L10 19.8l7.8-7.8L10 4.2z\" fill=\"currentColor\"/></svg></button>",
                esc(title)
            );
        }
        let _ = write!(
            out,
            "<a href=\"{}\" class=\"nav-list-link{}\"{}>{}</a>",
            esc(&site.page_url(item)),
            if here { " active" } else { "" },
            if here { " aria-current=\"page\"" } else { "" },
            esc(site.book.pages()[item].label())
        );
        if !children.is_empty() {
            nav_list(site, nav, children, current, trail, out);
        }
        out.push_str("</li>");
    }
    out.push_str("</ul>");
}

fn nav_html(site: &Site<'_>, nav: &Nav, current: usize) -> String {
    let trail = nav.trail(current);
    let mut out = String::new();
    nav_list(site, nav, nav.roots(), current, &trail, &mut out);
    out
}

fn breadcrumbs(site: &Site<'_>, current: usize) -> String {
    let trail = site.book.nav().trail(current);
    if trail.len() < 2 {
        return String::new();
    }
    let mut out = String::from(
        "<nav aria-label=\"Breadcrumb\" class=\"breadcrumb-nav\"><ol class=\"breadcrumb-nav-list\">",
    );
    for (k, &page) in trail.iter().enumerate() {
        let label = esc(site.book.pages()[page].label());
        if k + 1 == trail.len() {
            let _ = write!(
                out,
                "<li class=\"breadcrumb-nav-list-item\"><span>{label}</span></li>"
            );
        } else {
            let _ = write!(
                out,
                "<li class=\"breadcrumb-nav-list-item\"><a href=\"{}\">{label}</a></li>",
                esc(&site.page_url(page))
            );
        }
    }
    out.push_str("</ol></nav>");
    out
}

fn neighbours(site: &Site<'_>, current: usize) -> String {
    let (before, after) = site.book.nav().neighbours(current);
    if before.is_none() && after.is_none() {
        return String::new();
    }
    let mut out = String::from("<div class=\"page-footer\">");
    if let Some(page) = before {
        let _ = write!(
            out,
            "<a class=\"previous\" href=\"{}\"><small>Previous</small>{}</a>",
            esc(&site.page_url(page)),
            esc(site.book.pages()[page].label())
        );
    }
    if let Some(page) = after {
        let _ = write!(
            out,
            "<a class=\"next\" href=\"{}\"><small>Next</small>{}</a>",
            esc(&site.page_url(page)),
            esc(site.book.pages()[page].label())
        );
    }
    out.push_str("</div>");
    out
}

fn document(site: &Site<'_>, current: usize, content: &str) -> String {
    let page = &site.book.pages()[current];
    let title = page.title.as_deref().unwrap_or(&site.title);
    let window = if title == site.title {
        esc(title)
    } else {
        format!("{} - {}", esc(title), esc(&site.title))
    };
    let (brand, head, header) = if site.plain {
        (esc(&site.title), String::new(), String::new())
    } else {
        (
            TITLE.replace("{{ site.title }}", &esc(&site.title)),
            HEAD.to_string(),
            HEADER.to_string(),
        )
    };
    let css = site.url("assets/css/site.css");
    let js = site.url("assets/js/site.js");
    format!(
        "<!DOCTYPE html>\n<html lang=\"en\" data-fui-theme=\"dark\" data-fui-palette=\"green\">\n<head>\n<meta charset=\"utf-8\">\n\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<title>{window}</title>\n\
<link rel=\"stylesheet\" href=\"{css}\">\n{head}</head>\n<body>\n\
<a class=\"skip-to-main\" href=\"#main-content\">Skip to main content</a>\n\
<div class=\"side-bar\">\n<div class=\"site-header\"><a href=\"{home}\" class=\"site-title\">{brand}</a>\
<button id=\"menu-button\" class=\"site-button\" aria-label=\"Menu\" aria-expanded=\"false\"><svg class=\"icon\" viewBox=\"0 0 24 24\" aria-hidden=\"true\"><path d=\"M3 6h18v2H3zM3 11h18v2H3zM3 16h18v2H3z\"/></svg></button></div>\n\
<nav aria-label=\"Main\" id=\"site-nav\" class=\"site-nav\">{nav}</nav>\n</div>\n\
<div class=\"main\" id=\"top\">\n<div id=\"main-header\" class=\"main-header\">\n\
<div class=\"search\"><div class=\"search-input-wrap\"><svg class=\"icon\" viewBox=\"0 0 24 24\" aria-hidden=\"true\"><path d=\"M10 3a7 7 0 1 0 4.2 12.6l4.6 4.6 1.4-1.4-4.6-4.6A7 7 0 0 0 10 3zm0 2a5 5 0 1 1 0 10 5 5 0 0 1 0-10z\"/></svg>\
<input type=\"text\" id=\"search-input\" class=\"search-input\" placeholder=\"Search {book}\" aria-label=\"Search {book}\" autocomplete=\"off\"></div>\
<div id=\"search-results\" class=\"search-results\"></div></div>\n<div class=\"header-custom\">{header}</div>\n</div>\n\
<div id=\"main-content-wrap\" class=\"main-content-wrap\">\n{crumbs}<div id=\"main-content\" class=\"main-content\" role=\"main\">\n{content}\n{near}</div>\n</div>\n</div>\n\
<script src=\"{js}\"></script>\n</body>\n</html>\n",
        home = esc(&site.url("")),
        nav = nav_html(site, site.book.nav(), current),
        book = esc(&site.title),
        crumbs = breadcrumbs(site, current),
        near = neighbours(site, current),
    )
}

// ---------------------------------------------------------------- search

/// One thing the search finds: a page, or a section of one.
struct Entry {
    title: String,
    doc: String,
    content: String,
    url: String,
}

/// The entries of a page: the page itself (its text before the first heading) and each section below its headings.
fn entries(site: &Site<'_>, page: usize, html: &str, headings: &[Heading]) -> Vec<Entry> {
    let label = site.book.pages()[page].label().to_string();
    let url = site.page_url(page);
    let mut out = Vec::new();
    // The text between one heading and the next, found by the markers `decorate` wrote.
    let mut marks: Vec<(usize, &Heading)> = Vec::new();
    for heading in headings {
        if let Some(at) = html.find(&format!("<h{} id=\"{}\"", heading.level, heading.id)) {
            marks.push((at, heading));
        }
    }
    let first = marks.first().map_or(html.len(), |(at, _)| *at);
    out.push(Entry {
        title: label.clone(),
        doc: label.clone(),
        content: clip(&text_of(&html[..first])),
        url: url.clone(),
    });
    for (k, (at, heading)) in marks.iter().enumerate() {
        let end = marks.get(k + 1).map_or(html.len(), |(next, _)| *next);
        let body = &html[*at..end];
        // A heading of the page title itself adds nothing the page entry does not say.
        if heading.level == 1 && heading.text == label {
            continue;
        }
        let text = text_of(body);
        let content = text
            .strip_prefix(heading.text.as_str())
            .unwrap_or(&text)
            .trim_start_matches(" #")
            .trim();
        out.push(Entry {
            title: heading.text.clone(),
            doc: label.clone(),
            content: clip(content),
            url: format!("{url}#{}", heading.id),
        });
    }
    out
}

/// At most this much of a section is searched.
const SEARCHED: usize = 4000;

fn clip(text: &str) -> String {
    text.chars().take(SEARCHED).collect()
}

fn search_data(entries: &[Entry]) -> String {
    let mut map = quench_serde::Map::new();
    for (i, entry) in entries.iter().enumerate() {
        let mut one = quench_serde::Map::new();
        one.insert("id", Value::from(i.to_string()));
        one.insert("title", Value::from(entry.title.as_str()));
        one.insert("content", Value::from(entry.content.as_str()));
        one.insert("url", Value::from(entry.url.as_str()));
        one.insert("doc", Value::from(entry.doc.as_str()));
        map.insert(i.to_string(), Value::Object(one));
    }
    quench_serde::json::to_string_pretty(&Value::Object(map))
}

// ---------------------------------------------------------------- the files that are not pages

/// What Jekyll leaves out of a site without being told: names starting with `_`, `.` or `#`, and a Gemfile.
fn left_out(name: &str) -> bool {
    name.starts_with(['_', '.', '#']) || matches!(name, "Gemfile" | "Gemfile.lock" | "node_modules")
}

fn excluded(config: Option<&Value>) -> BTreeSet<String> {
    config
        .and_then(|c| c.get("exclude"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(|s| s.trim_end_matches('/').to_string())
        .collect()
}

/// Copies the files of the book that are not pages: the other files as they are, a Markdown file without front matter too.
fn copy_files(
    dir: &Path,
    base: &Path,
    out: &Path,
    excluded: &BTreeSet<String>,
    count: &mut usize,
) -> Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .flatten()
        .map(|e| e.path())
        .collect();
    entries.sort();
    for path in entries {
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let rel = path
            .strip_prefix(base)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        if left_out(name) || excluded.contains(&rel) {
            continue;
        }
        if path.is_dir() {
            copy_files(&path, base, out, excluded, count)?;
            continue;
        }
        if path.extension().is_some_and(|e| e == "md") {
            let text = std::fs::read_to_string(&path)
                .with_context(|| format!("reading {}", path.display()))?;
            if split_front_matter(&text).is_some() {
                continue;
            }
        }
        let target = out.join(&rel);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        std::fs::copy(&path, &target).with_context(|| format!("copying {}", path.display()))?;
        *count += 1;
    }
    Ok(())
}

fn write(out: &Path, rel: &str, text: &str) -> Result<()> {
    let target = out.join(rel);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    std::fs::write(&target, text).with_context(|| format!("writing {}", target.display()))
}

/// The base path in the form the site's addresses use: a `/` in front, none behind, empty for none.
fn base_of(baseurl: &str) -> String {
    let trimmed = baseurl.trim().trim_matches('/');
    if trimmed.is_empty() {
        String::new()
    } else {
        format!("/{trimmed}")
    }
}

/// Writes the site of the book in `dir` into `out`.
///
/// # Errors
/// The book cannot be read, two pages are written to the same place, or a file cannot be written.
pub fn build_site(dir: &Path, out: &Path, options: &SiteOptions) -> Result<Written> {
    let book = Book::open(dir)?;
    let config_file = dir.join("_config.yml");
    let config: Option<Value> = if config_file.is_file() {
        let text = std::fs::read_to_string(&config_file)
            .with_context(|| format!("reading {}", config_file.display()))?;
        Some(
            quench_serde::yaml::from_str(&text)
                .with_context(|| format!("{} is not valid YAML", config_file.display()))?,
        )
    } else {
        None
    };

    let places: Vec<(String, String)> = book.pages().iter().map(location).collect();
    let mut seen = BTreeSet::new();
    for (page, (file, _)) in book.pages().iter().zip(&places) {
        if !seen.insert(file.clone()) {
            bail!("{}: another page is written to {file} as well", page.path);
        }
    }
    let liquid_flags: Vec<bool> = book
        .pages()
        .iter()
        .map(|page| {
            // The text as written, because the front matter may switch Liquid off for the page.
            let text = std::fs::read_to_string(dir.join(&page.path)).unwrap_or_default();
            liquid_on(config.as_ref(), &page.path, &text)
        })
        .collect();
    let title = book
        .title()
        .map_or_else(|| "Documentation".to_string(), str::to_string);
    let site = Site {
        book: &book,
        title,
        baseurl: if options.baseurl.is_empty() {
            base_of(book.baseurl())
        } else {
            base_of(&options.baseurl)
        },
        plain: options.plain,
        places,
        liquid: liquid_flags,
    };

    std::fs::create_dir_all(out).with_context(|| format!("creating {}", out.display()))?;
    let mut found = Vec::new();
    for (i, page) in book.pages().iter().enumerate() {
        let body = if site.liquid[i] {
            liquid(&page.body, &site, page.label())
        } else {
            page.body.clone()
        };
        let html = markdown::to_html_with(&body, Options::GFM);
        let (html, headings) = decorate(&html);
        let html = link_pages(&html, &site, i);
        found.extend(entries(&site, i, &html, &headings));
        write(out, &site.places[i].0, &document(&site, i, &html))?;
    }

    let css = if options.plain {
        LAYOUT_CSS.to_string()
    } else {
        format!("{LAYOUT_CSS}\n{TOKENS_CSS}\n{FORGE_CSS}")
    };
    write(out, "assets/css/site.css", &css)?;
    write(
        out,
        "assets/js/site.js",
        &SITE_JS.replace("__SEARCH_URL__", &site.url("assets/js/search-data.json")),
    )?;
    write(out, "assets/js/search-data.json", &search_data(&found))?;

    let mut files = 3;
    copy_files(dir, dir, out, &excluded(config.as_ref()), &mut files)?;
    Ok(Written {
        pages: book.pages().len(),
        files: files + book.pages().len(),
    })
}

/// `text` as the first-found heading anchor would be: exposed for the tests.
#[must_use]
pub fn heading_slug(text: &str) -> String {
    slug(text)
}

/// The HTML of some Markdown with its headings decorated: exposed for the tests.
#[must_use]
pub fn decorated(markdown_text: &str) -> String {
    decorate(&markdown::to_html_with(markdown_text, Options::GFM)).0
}
