//! `anvil docs`: checks, builds and releases documentation books.
//!
//! A book is a directory of Markdown files that Jekyll turns into a site (`just-the-docs` navigation from front
//! matter). Anvil
//!
//! * **checks** a book without Jekyll: every page has a `title`, every `parent` names a page that has children, the
//!   `nav_order` values of siblings differ, every relative link resolves, and every `openapi.yaml` describes exactly
//!   the routes in the source it documents;
//! * **builds** it: the checks, then `jekyll build`, then a reproducible `.tar.gz` of the site and a `manifest.json`;
//! * **releases** it: the archive and the manifest go to a warehouse files storage under
//!   `<book>/<version>/`, and `<book>/latest.json` is rewritten to point at them, which is what a preview service
//!   reads.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use quench_core::{Context, Result, bail};
use quench_serde::{Map, Value};

use crate::commands::artifact::{Answer, Upload, read_version, send};
use crate::config::{Config, DocsBook, DocsOpenApi};

/// What `anvil docs build` and `anvil docs release` were asked.
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// Use the theme the book's `_config.yml` names (`remote_theme`: needs the `jekyll-remote-theme` gem and the
    /// network) instead of the installed `just-the-docs` gem.
    pub remote_theme: bool,
    /// Skip the checks.
    pub no_check: bool,
    /// Where the builds go, if not `[docs].out`.
    pub out: Option<String>,
    /// The `baseurl` to build for, if not the book's.
    pub baseurl: Option<String>,
    /// Release: build and check everything, send nothing.
    pub dry_run: bool,
}

/// What a build left.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Built {
    pub book: String,
    pub version: String,
    pub site: PathBuf,
    pub archive: PathBuf,
    pub manifest: PathBuf,
    pub files: usize,
    pub bytes: usize,
    /// The `manifest.json` text.
    pub manifest_text: String,
}

// ---------------------------------------------------------------- pages

/// A Markdown page of a book, as far as the navigation is concerned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    /// From the book's directory, with `/`.
    pub path: String,
    pub title: Option<String>,
    pub parent: Option<String>,
    pub grand_parent: Option<String>,
    pub nav_order: Option<i64>,
    pub has_children: bool,
    pub body: String,
}

/// The front matter and the rest of a Markdown file, if it starts with `---`.
#[must_use]
pub fn split_front_matter(text: &str) -> Option<(&str, &str)> {
    let rest = text.strip_prefix("---\n")?;
    let end = if rest.starts_with("---\n") {
        0
    } else {
        rest.find("\n---\n")? + 1
    };
    let head = &rest[..end];
    let body = rest[end..].strip_prefix("---\n").unwrap_or("");
    Some((head, body))
}

fn exclusions(dir: &Path) -> Result<BTreeSet<String>> {
    let mut out = BTreeSet::new();
    let file = dir.join("_config.yml");
    if !file.is_file() {
        return Ok(out);
    }
    let text =
        std::fs::read_to_string(&file).with_context(|| format!("reading {}", file.display()))?;
    let value: Value = quench_serde::yaml::from_str(&text)
        .with_context(|| format!("{} is not valid YAML", file.display()))?;
    if let Some(Value::Array(list)) = value.get("exclude") {
        for item in list {
            if let Some(s) = item.as_str() {
                out.insert(s.trim_end_matches('/').to_string());
            }
        }
    }
    Ok(out)
}

fn walk(
    dir: &Path,
    base: &Path,
    excluded: &BTreeSet<String>,
    out: &mut Vec<PathBuf>,
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
        if name.starts_with('_') || name.starts_with('.') || excluded.contains(&rel) {
            continue;
        }
        if path.is_dir() {
            walk(&path, base, excluded, out)?;
        } else {
            out.push(path);
        }
    }
    Ok(())
}

/// Every Markdown page of the book, and the problems found while reading them (front matter that is not YAML).
///
/// # Errors
/// The directory cannot be read.
pub fn read_pages(dir: &Path) -> Result<(Vec<Page>, Vec<String>)> {
    let excluded = exclusions(dir)?;
    let mut files = Vec::new();
    walk(dir, dir, &excluded, &mut files)?;
    let mut pages = Vec::new();
    let mut problems = Vec::new();
    for file in files {
        if file.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let rel = file
            .strip_prefix(dir)
            .unwrap_or(&file)
            .to_string_lossy()
            .replace('\\', "/");
        let text = std::fs::read_to_string(&file)
            .with_context(|| format!("reading {}", file.display()))?;
        let Some((head, body)) = split_front_matter(&text) else {
            // Jekyll copies a Markdown file without front matter as it is: it is not a page.
            problems.push(format!(
                "{rel}: no front matter (Jekyll would not render it as a page)"
            ));
            continue;
        };
        let value: Value = match quench_serde::yaml::from_str(head) {
            Ok(v) => v,
            Err(e) => {
                problems.push(format!("{rel}: the front matter is not valid YAML: {e}"));
                continue;
            }
        };
        let text_of = |key: &str| value.get(key).and_then(Value::as_str).map(str::to_string);
        pages.push(Page {
            path: rel,
            title: text_of("title"),
            parent: text_of("parent"),
            grand_parent: text_of("grand_parent"),
            nav_order: value.get("nav_order").and_then(Value::as_i64),
            has_children: matches!(value.get("has_children"), Some(Value::Bool(true))),
            body: body.to_string(),
        });
    }
    Ok((pages, problems))
}

// ---------------------------------------------------------------- links

/// The link targets of a Markdown body: `[text](target)` outside fenced blocks and code spans.
#[must_use]
pub fn links(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut fenced = false;
    for line in body.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            continue;
        }
        // Code spans hold examples, not links.
        let mut plain = String::new();
        let mut in_code = false;
        for c in line.chars() {
            if c == '`' {
                in_code = !in_code;
            } else if !in_code {
                plain.push(c);
            }
        }
        let mut rest = plain.as_str();
        while let Some(at) = rest.find("](") {
            rest = &rest[at + 2..];
            let end = rest
                .find(|c: char| c == ')' || c.is_whitespace())
                .unwrap_or(rest.len());
            out.push(rest[..end].to_string());
            rest = &rest[end..];
        }
    }
    out
}

/// Whether a link leaves the book (or is not a path at all).
fn outside(target: &str) -> bool {
    target.is_empty()
        || target.starts_with('#')
        || target.contains('\\')
        || target.contains('|')
        || target
            .split('/')
            .next()
            .is_some_and(|first| first.contains(':'))
}

fn normalise(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn link_problems(dir: &Path, page: &Page) -> Vec<String> {
    let mut out = Vec::new();
    let from = Path::new(&page.path)
        .parent()
        .unwrap_or_else(|| Path::new(""));
    for target in links(&page.body) {
        if outside(&target) {
            continue;
        }
        let path = target.split(['#', '?']).next().unwrap_or_default();
        if path.is_empty() {
            continue;
        }
        let joined = path
            .strip_prefix('/')
            .map_or_else(|| from.join(path), PathBuf::from);
        let full = dir.join(normalise(&joined));
        let found = full.is_file() || (full.is_dir() && full.join("index.md").is_file());
        if !found {
            out.push(format!("{}: broken link `{target}`", page.path));
        }
    }
    out
}

// ---------------------------------------------------------------- navigation

/// A parent, its parent, and a `nav_order`: the pages that share all three are in no defined order.
type Siblings<'a> = (Option<&'a str>, Option<&'a str>, i64);

fn nav_problems(pages: &[Page]) -> Vec<String> {
    let mut out = Vec::new();
    let mut by_title: BTreeMap<&str, Vec<&Page>> = BTreeMap::new();
    for page in pages {
        match &page.title {
            Some(title) => by_title.entry(title.as_str()).or_default().push(page),
            None => out.push(format!("{}: no `title` in the front matter", page.path)),
        }
    }
    for page in pages {
        let Some(parent) = page.parent.as_deref() else {
            continue;
        };
        let candidates: Vec<&&Page> = by_title
            .get(parent)
            .map(|c| c.iter().filter(|p| p.has_children).collect())
            .unwrap_or_default();
        match candidates.len() {
            0 => {
                let any = by_title.contains_key(parent);
                out.push(if any {
                    format!(
                        "{}: parent `{parent}` exists but has no `has_children: true`",
                        page.path
                    )
                } else {
                    format!(
                        "{}: parent `{parent}` is not the title of any page",
                        page.path
                    )
                });
            }
            1 => {}
            _ => {
                let narrowed = candidates
                    .iter()
                    .filter(|p| p.parent == page.grand_parent)
                    .count();
                if page.grand_parent.is_none() || narrowed != 1 {
                    out.push(format!(
                        "{}: parent `{parent}` is ambiguous ({} pages with that title have children): add `grand_parent`",
                        page.path,
                        candidates.len()
                    ));
                }
            }
        }
    }
    // Siblings with the same `nav_order` are shown in no defined order.
    let mut groups: BTreeMap<Siblings<'_>, Vec<&str>> = BTreeMap::new();
    for page in pages {
        if let Some(order) = page.nav_order {
            groups
                .entry((page.parent.as_deref(), page.grand_parent.as_deref(), order))
                .or_default()
                .push(&page.path);
        }
    }
    for ((parent, _, order), paths) in groups {
        if paths.len() > 1 {
            out.push(format!(
                "nav_order {order} is used by {} pages under {}: {}",
                paths.len(),
                parent.unwrap_or("the top level"),
                paths.join(", ")
            ));
        }
    }
    out
}

// ---------------------------------------------------------------- OpenAPI

const METHODS: [&str; 7] = ["get", "post", "put", "delete", "patch", "head", "options"];

/// `/x/:id` and `/x/{id:.+}` are the route `/x/{id}`.
#[must_use]
pub fn normalise_route(path: &str) -> String {
    let mut out = String::new();
    let mut chars = path.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' => {
                let mut name = String::new();
                for n in chars.by_ref() {
                    if n == '}' {
                        break;
                    }
                    name.push(n);
                }
                let name = name.split(':').next().unwrap_or_default();
                out.push('{');
                out.push_str(name);
                out.push('}');
            }
            ':' if chars
                .peek()
                .is_some_and(|n| n.is_ascii_alphabetic() || *n == '_') =>
            {
                let mut name = String::new();
                while let Some(n) = chars.peek() {
                    if n.is_ascii_alphanumeric() || *n == '_' {
                        name.push(*n);
                        chars.next();
                    } else {
                        break;
                    }
                }
                out.push('{');
                out.push_str(&name);
                out.push('}');
            }
            other => out.push(other),
        }
    }
    out
}

/// The `(method, path)` pairs a Rust source declares: `#[get("/x")]` and
/// `RouteRegistration { method: Method::HEAD, pattern: "/x" }`.
#[must_use]
pub fn source_routes(text: &str) -> BTreeSet<(String, String)> {
    let mut found = BTreeSet::new();
    let mut i = 0;
    while let Some(at) = text[i..].find("#[") {
        let start = i + at + 2;
        i = start;
        let rest = text[start..].trim_start();
        let Some(method) = METHODS.iter().find(|m| {
            rest.strip_prefix(**m)
                .is_some_and(|after| after.trim_start().starts_with('('))
        }) else {
            continue;
        };
        let after = rest[method.len()..].trim_start()[1..].trim_start();
        if let Some(quoted) = after.strip_prefix('"')
            && let Some(end) = quoted.find('"')
        {
            found.insert(((*method).to_string(), normalise_route(&quoted[..end])));
        }
    }
    let mut i = 0;
    while let Some(at) = text[i..].find("Method::") {
        let start = i + at + "Method::".len();
        i = start;
        let name: String = text[start..]
            .chars()
            .take_while(char::is_ascii_alphabetic)
            .collect();
        let method = name.to_ascii_lowercase();
        if !METHODS.contains(&method.as_str()) {
            continue;
        }
        let rest = text[start + name.len()..].trim_start();
        let Some(rest) = rest.strip_prefix(',') else {
            continue;
        };
        let Some(rest) = rest.trim_start().strip_prefix("pattern") else {
            continue;
        };
        let Some(rest) = rest.trim_start().strip_prefix(':') else {
            continue;
        };
        if let Some(quoted) = rest.trim_start().strip_prefix('"')
            && let Some(end) = quoted.find('"')
        {
            found.insert((method, normalise_route(&quoted[..end])));
        }
    }
    found
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = read.flatten().map(|e| e.path()).collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n == "target") {
                continue;
            }
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

fn collect_refs(value: &Value, out: &mut BTreeSet<String>) {
    match value {
        Value::Object(map) => {
            for (key, inner) in map.iter() {
                if key == "$ref"
                    && let Some(s) = inner.as_str()
                {
                    out.insert(s.to_string());
                } else {
                    collect_refs(inner, out);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_refs(item, out);
            }
        }
        _ => {}
    }
}

fn resolves(doc: &Value, reference: &str) -> bool {
    let Some(pointer) = reference.strip_prefix('#') else {
        return true;
    };
    doc.pointer(pointer).is_some()
}

/// The problems of one `OpenAPI` document against the routes in `sources`.
#[must_use]
pub fn openapi_problems(name: &str, spec: &str, sources: &[PathBuf]) -> Vec<String> {
    let mut out = Vec::new();
    let doc: Value = match quench_serde::yaml::from_str(spec) {
        Ok(v) => v,
        Err(e) => return vec![format!("{name}: openapi.yaml is not valid YAML: {e}")],
    };
    let version = doc
        .get("openapi")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !version.starts_with("3.1") {
        out.push(format!("{name}: `openapi` must be 3.1.x, is `{version}`"));
    }
    if doc.get("info").is_none() {
        out.push(format!("{name}: no `info`"));
    }
    let mut documented = BTreeSet::new();
    match doc.get("paths") {
        Some(Value::Object(paths)) => {
            for (path, item) in paths.iter() {
                if let Value::Object(item) = item {
                    for method in item.keys() {
                        if METHODS.contains(&method.as_str()) {
                            documented.insert((method.clone(), normalise_route(path)));
                        }
                    }
                }
            }
        }
        _ => out.push(format!("{name}: no `paths`")),
    }
    let mut routes = BTreeSet::new();
    let mut files = Vec::new();
    for source in sources {
        rust_files(source, &mut files);
    }
    for file in files {
        if let Ok(text) = std::fs::read_to_string(&file) {
            routes.extend(source_routes(&text));
        }
    }
    for (method, path) in routes.difference(&documented) {
        out.push(format!(
            "{name}: route in source but not documented: {} {path}",
            method.to_ascii_uppercase()
        ));
    }
    for (method, path) in documented.difference(&routes) {
        out.push(format!(
            "{name}: documented but no route in source: {} {path}",
            method.to_ascii_uppercase()
        ));
    }
    let mut refs = BTreeSet::new();
    collect_refs(&doc, &mut refs);
    for reference in refs {
        if !resolves(&doc, &reference) {
            out.push(format!("{name}: dangling $ref {reference}"));
        }
    }
    out
}

#[allow(clippy::literal_string_with_formatting_args)] // `{name}` is the service, not a format argument
fn book_openapi_problems(root: &Path, dir: &Path, openapi: &DocsOpenApi) -> Vec<String> {
    let mut out = Vec::new();
    let specs = dir.join(&openapi.specs);
    let Ok(read) = std::fs::read_dir(&specs) else {
        return vec![format!("openapi: {} does not exist", specs.display())];
    };
    let mut names: Vec<String> = read
        .flatten()
        .filter(|e| e.path().join("openapi.yaml").is_file())
        .filter_map(|e| e.file_name().to_str().map(str::to_string))
        .collect();
    names.sort();
    for name in names {
        let Ok(text) = std::fs::read_to_string(specs.join(&name).join("openapi.yaml")) else {
            continue;
        };
        let sources: Vec<PathBuf> = openapi
            .sources
            .iter()
            .map(|s| root.join(s.replace("{name}", &name)))
            .collect();
        out.extend(openapi_problems(&name, &text, &sources));
    }
    out
}

// ---------------------------------------------------------------- check

/// Every problem of a book: pages, navigation, links and `OpenAPI` documents.
///
/// # Errors
/// The book's directory cannot be read.
pub fn check_book(root: &Path, book: &DocsBook) -> Result<Vec<String>> {
    let dir = root.join(&book.dir);
    if !dir.is_dir() {
        bail!("the book's directory {} does not exist", dir.display());
    }
    let (pages, mut problems) = read_pages(&dir)?;
    problems.extend(nav_problems(&pages));
    for page in &pages {
        problems.extend(link_problems(&dir, page));
    }
    if let Some(openapi) = &book.openapi {
        problems.extend(book_openapi_problems(root, &dir, openapi));
    }
    Ok(problems)
}

fn select<'a>(config: &'a Config, name: Option<&str>) -> Result<Vec<(&'a str, &'a DocsBook)>> {
    let books = &config.docs.books;
    if books.is_empty() {
        bail!("no books: add [docs.books.<name>] to .anvil.toml or a file in .anvil/");
    }
    match name {
        Some(name) => {
            let (key, book) = books.get_key_value(name).with_context(|| {
                let known: Vec<&str> = books.keys().map(String::as_str).collect();
                format!("no book `{name}` (known: {})", known.join(", "))
            })?;
            Ok(vec![(key.as_str(), book)])
        }
        None => Ok(books.iter().map(|(k, v)| (k.as_str(), v)).collect()),
    }
}

/// `anvil docs check`: prints the problems of the books and fails if there are any.
///
/// # Errors
/// An unknown book, an unreadable directory, or problems in a book.
pub fn check(config: &Config, root: &Path, name: Option<&str>, out: &mut dyn Write) -> Result<()> {
    let mut total = 0;
    for (key, book) in select(config, name)? {
        let problems = check_book(root, book)?;
        if problems.is_empty() {
            writeln!(out, "ok   {key}")?;
        } else {
            writeln!(out, "FAIL {key}")?;
            for problem in &problems {
                writeln!(out, "     {problem}")?;
            }
        }
        total += problems.len();
    }
    if total > 0 {
        bail!("{total} problem(s) in the documentation");
    }
    Ok(())
}

/// `anvil docs list`: the books and where they come from.
///
/// # Errors
/// Output failed.
pub fn list(config: &Config, out: &mut dyn Write) -> Result<()> {
    if config.docs.books.is_empty() {
        writeln!(
            out,
            "no books: add [docs.books.<name>] to .anvil.toml or a file in .anvil/"
        )?;
        return Ok(());
    }
    for (name, book) in &config.docs.books {
        writeln!(
            out,
            "{name}  {}  {}",
            book.dir,
            book.title.as_deref().unwrap_or(name)
        )?;
    }
    Ok(())
}

// ---------------------------------------------------------------- build

fn today() -> String {
    quench_core::time::Time::now().format("%Y-%m-%d")
}

fn version_of(root: &Path, book: &DocsBook) -> Result<String> {
    if let Some(version) = book.version.as_deref().filter(|v| !v.is_empty()) {
        return Ok(version.to_string());
    }
    if let Some(file) = &book.version_file {
        return read_version(&root.join(file));
    }
    Ok(today())
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

fn site_files(site: &Path) -> Result<Vec<(String, Vec<u8>)>> {
    fn visit(dir: &Path, base: &Path, out: &mut Vec<(String, Vec<u8>)>) -> Result<()> {
        let mut entries: Vec<_> = std::fs::read_dir(dir)
            .with_context(|| format!("reading {}", dir.display()))?
            .flatten()
            .map(|e| e.path())
            .collect();
        entries.sort();
        for path in entries {
            if path.is_dir() {
                visit(&path, base, out)?;
            } else {
                let rel = path
                    .strip_prefix(base)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                let data =
                    std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
                out.push((rel, data));
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    visit(site, site, &mut out)?;
    Ok(out)
}

fn jekyll_command(config: &Config) -> Vec<String> {
    config
        .docs
        .jekyll
        .as_deref()
        .filter(|c| !c.trim().is_empty())
        .unwrap_or("jekyll")
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

/// `[text](target.md#part)` as `[text](target.html#part)`, outside fenced blocks and code spans.
///
/// Only the targets that stay inside the book change. Jekyll writes `page.md` as `page.html`; the
/// `jekyll-relative-links` plugin would do this, and building without it must not leave links to Markdown files.
#[must_use]
pub fn rewrite_links(body: &str) -> String {
    let mut out = String::with_capacity(body.len());
    let mut fenced = false;
    for line in body.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fenced = !fenced;
            out.push_str(line);
            continue;
        }
        if fenced {
            out.push_str(line);
            continue;
        }
        let mut in_code = false;
        let mut rest = line;
        while let Some(c) = rest.chars().next() {
            if c == '`' {
                in_code = !in_code;
            }
            if !in_code && rest.starts_with("](") {
                let after = &rest[2..];
                let end = after
                    .find(|c: char| c == ')' || c.is_whitespace())
                    .unwrap_or(after.len());
                let target = &after[..end];
                let (path, tail) = target
                    .find(['#', '?'])
                    .map_or((target, ""), |at| (&target[..at], &target[at..]));
                out.push_str("](");
                if !outside(target) && path.to_ascii_lowercase().ends_with(".md") {
                    out.push_str(&path[..path.len() - 3]);
                    out.push_str(".html");
                    out.push_str(tail);
                } else {
                    out.push_str(target);
                }
                rest = &after[end..];
                continue;
            }
            out.push(c);
            rest = &rest[c.len_utf8()..];
        }
    }
    out
}

fn copy_staged(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to).with_context(|| format!("creating {}", to.display()))?;
    let mut entries: Vec<_> = std::fs::read_dir(from)
        .with_context(|| format!("reading {}", from.display()))?
        .flatten()
        .map(|e| e.path())
        .collect();
    entries.sort();
    for path in entries {
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if matches!(name, ".git" | ".jekyll-cache" | "_site" | ".sass-cache") {
            continue;
        }
        let target = to.join(name);
        if path.is_dir() {
            copy_staged(&path, &target)?;
        } else if path.extension().is_some_and(|e| e == "md") {
            let text = std::fs::read_to_string(&path)
                .with_context(|| format!("reading {}", path.display()))?;
            let text = match split_front_matter(&text) {
                // Only a page (front matter) is rendered; a Markdown file without it is copied as it is.
                Some((head, body)) => format!("---\n{head}---\n{}", rewrite_links(body)),
                None => text,
            };
            std::fs::write(&target, text)
                .with_context(|| format!("writing {}", target.display()))?;
        } else {
            std::fs::copy(&path, &target).with_context(|| format!("copying {}", path.display()))?;
        }
    }
    Ok(())
}

/// Copies the book to `stage` with the links between its pages pointing at the pages Jekyll writes.
fn stage_book(dir: &Path, stage: &Path) -> Result<()> {
    if stage.is_dir() {
        std::fs::remove_dir_all(stage).with_context(|| format!("clearing {}", stage.display()))?;
    }
    copy_staged(dir, stage)
}

/// What Jekyll says when a gem is missing, as advice.
fn missing_gem_hint(stderr: &str) -> Option<String> {
    let marker = "cannot load such file -- ";
    let at = stderr.find(marker)?;
    let gem: String = stderr[at + marker.len()..]
        .chars()
        .take_while(|c| !c.is_whitespace() && *c != '(')
        .collect();
    Some(format!("a Ruby gem is missing: `gem install {gem}`"))
}

fn run_jekyll(
    config: &Config,
    source: &Path,
    site: &Path,
    out_dir: &Path,
    options: &Options,
    baseurl: Option<&str>,
    log: &mut dyn Write,
) -> Result<()> {
    let command = jekyll_command(config);
    let Some((program, fixed)) = command.split_first() else {
        bail!("`[docs].jekyll` is empty");
    };
    let probe = Command::new(program).args(fixed).arg("--version").output();
    if probe.as_ref().map_or(true, |o| !o.status.success()) {
        bail!(
            "could not run `{}`: install Jekyll (`gem install jekyll`), or set `jekyll` under [docs] (`bundle exec jekyll`)",
            command.join(" ")
        );
    }
    // The staged copy has its links rewritten, so `jekyll-relative-links` is never needed. The theme is the
    // installed `just-the-docs` gem unless the book's own (remote) theme is asked for.
    let mut configs = Vec::new();
    if source.join("_config.yml").is_file() {
        configs.push(source.join("_config.yml"));
    }
    let override_file = out_dir.join("jekyll-override.yml");
    let plugins = if options.remote_theme {
        "plugins:\n  - jekyll-remote-theme\n  - jekyll-seo-tag\n"
    } else {
        "remote_theme: null\ntheme: just-the-docs\nplugins:\n  - jekyll-seo-tag\n"
    };
    std::fs::write(&override_file, plugins)
        .with_context(|| format!("writing {}", override_file.display()))?;
    configs.push(override_file);
    let mut cmd = Command::new(program);
    cmd.args(fixed)
        .arg("build")
        .arg("--source")
        .arg(source)
        .arg("--disable-disk-cache")
        .arg("--destination")
        .arg(site);
    let list: Vec<String> = configs
        .iter()
        .map(|c| c.to_string_lossy().into_owned())
        .collect();
    cmd.arg("--config").arg(list.join(","));
    if let Some(baseurl) = baseurl {
        cmd.arg("--baseurl").arg(baseurl);
    }
    cmd.env("JEKYLL_ENV", "production");
    writeln!(
        log,
        "running: {} build {}",
        command.join(" "),
        source.display()
    )?;
    let output = cmd
        .output()
        .with_context(|| format!("running `{}`", command.join(" ")))?;
    if !output.status.success() {
        let text = String::from_utf8_lossy(&output.stderr);
        let lines: Vec<&str> = text.lines().collect();
        let tail = &lines[lines.len().saturating_sub(25)..];
        let hint = missing_gem_hint(&text)
            .map(|h| format!("\n{h}"))
            .unwrap_or_default();
        bail!(
            "jekyll failed ({}):\n{}{hint}",
            output.status,
            tail.join("\n")
        );
    }
    Ok(())
}

fn json_string(value: &Value) -> String {
    quench_serde::json::to_string_pretty(value)
}

/// Checks, builds and packages one book.
///
/// # Errors
/// An unknown book, a failed check, Jekyll missing or failing, files that cannot be read or written.
pub fn build_book(
    config: &Config,
    root: &Path,
    name: &str,
    options: &Options,
    log: &mut dyn Write,
) -> Result<Built> {
    let (key, book) = select(config, Some(name))?.remove(0);
    let dir = root.join(&book.dir);
    if !options.no_check {
        let problems = check_book(root, book)?;
        if !problems.is_empty() {
            for problem in &problems {
                writeln!(log, "     {problem}")?;
            }
            bail!(
                "{} problem(s) in the `{key}` book: fix them, or build with --no-check",
                problems.len()
            );
        }
        writeln!(log, "ok   {key}")?;
    }
    let version = version_of(root, book)?;
    let out_root = options
        .out
        .as_deref()
        .or(config.docs.out.as_deref())
        .unwrap_or("target/docs");
    let out_dir = root.join(out_root).join(key);
    let site = out_dir.join("site");
    if site.is_dir() {
        std::fs::remove_dir_all(&site).with_context(|| format!("clearing {}", site.display()))?;
    }
    std::fs::create_dir_all(&out_dir).with_context(|| format!("creating {}", out_dir.display()))?;
    let baseurl = options.baseurl.as_deref().or(book.baseurl.as_deref());
    let stage = out_dir.join("src");
    stage_book(&dir, &stage)?;
    run_jekyll(config, &stage, &site, &out_dir, options, baseurl, log)?;

    let files = site_files(&site)?;
    if files.is_empty() {
        bail!("jekyll built nothing into {}", site.display());
    }
    let bytes: usize = files.iter().map(|(_, d)| d.len()).sum();
    let mut tar = quench_archive::tar::Builder::new();
    for (path, data) in &files {
        tar.append_file(path, data)
            .with_context(|| format!("adding {path} to the archive"))?;
    }
    let archive_bytes = quench_archive::gzip::compress(&tar.finish());
    let archive_name = format!("{key}-{version}.tar.gz");
    let archive = out_dir.join(&archive_name);
    std::fs::write(&archive, &archive_bytes)
        .with_context(|| format!("writing {}", archive.display()))?;

    let mut manifest = Map::new();
    manifest.insert("book", Value::from(key));
    manifest.insert("title", Value::from(book.title.as_deref().unwrap_or(key)));
    manifest.insert("version", Value::from(version.as_str()));
    manifest.insert(
        "built",
        Value::from(quench_core::time::Time::now().to_rfc3339()),
    );
    manifest.insert("archive", Value::from(archive_name.as_str()));
    manifest.insert("archive_bytes", Value::from(archive_bytes.len() as u64));
    manifest.insert(
        "archive_sha256",
        Value::from(hex(&quench_crypto::hash::sha256(&archive_bytes))),
    );
    manifest.insert("files", Value::from(files.len() as u64));
    manifest.insert("site_bytes", Value::from(bytes as u64));
    if let Some(baseurl) = baseurl.filter(|b| !b.is_empty()) {
        manifest.insert("baseurl", Value::from(baseurl));
    }
    let manifest_text = json_string(&Value::Object(manifest));
    let manifest_path = out_dir.join("manifest.json");
    std::fs::write(&manifest_path, &manifest_text)
        .with_context(|| format!("writing {}", manifest_path.display()))?;
    writeln!(
        log,
        "built {key} {version}: {} files, {bytes} bytes -> {}",
        files.len(),
        archive.display()
    )?;
    Ok(Built {
        book: key.to_string(),
        version,
        site,
        archive,
        manifest: manifest_path,
        files: files.len(),
        bytes,
        manifest_text,
    })
}

/// `anvil docs build`.
///
/// # Errors
/// See [`build_book`].
pub fn build(
    config: &Config,
    root: &Path,
    name: Option<&str>,
    options: &Options,
    log: &mut dyn Write,
) -> Result<Vec<Built>> {
    let mut built = Vec::new();
    for (key, _) in select(config, name)? {
        built.push(build_book(config, root, key, options, log)?);
    }
    Ok(built)
}

// ---------------------------------------------------------------- release

fn encode_path(path: &str) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    for b in path.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~' | b'/') {
            out.push(b as char);
        } else {
            let _ = write!(out, "%{b:02X}");
        }
    }
    out
}

/// Where a book's files go: `<book>/<version>/<archive>`, `<book>/<version>/manifest.json`, `<book>/latest.json`.
#[must_use]
pub fn destinations(built: &Built) -> [String; 3] {
    let name = built
        .archive
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    [
        format!("{}/{}/{name}", built.book, built.version),
        format!("{}/{}/manifest.json", built.book, built.version),
        format!("{}/latest.json", built.book),
    ]
}

/// The `latest.json` text: the manifest, and where the files are in the storage.
fn latest_text(built: &Built, paths: &[String; 3]) -> Result<String> {
    let mut value: Value =
        quench_serde::json::from_str(&built.manifest_text).context("reading the manifest back")?;
    if let Value::Object(map) = &mut value {
        map.insert("archive_path", Value::from(paths[0].as_str()));
        map.insert("manifest_path", Value::from(paths[1].as_str()));
    }
    Ok(json_string(&value))
}

/// Builds the books and sends them to the warehouse; the token and the transport are the caller's, so a test can stand in
/// for the warehouse.
///
/// # Errors
/// See [`build_book`]; no warehouse or storage configured; a refusal from the warehouse.
pub fn release_with(
    config: &Config,
    root: &Path,
    name: Option<&str>,
    options: &Options,
    log: &mut dyn Write,
    token: &dyn Fn() -> Result<String>,
    put: &dyn Fn(&Upload) -> Result<Answer>,
) -> Result<()> {
    let warehouse = config
        .docs
        .warehouse_url
        .as_deref()
        .or(config.artifacts.warehouse_url.as_deref())
        .filter(|u| !u.is_empty())
        .context("no warehouse: set `warehouse_url` under [docs] (or [artifacts]) in .anvil.toml or .anvil/")?
        .trim_end_matches('/')
        .to_string();
    let storage = config
        .docs
        .storage
        .as_deref()
        .filter(|s| !s.is_empty())
        .context("no storage: set `storage` under [docs] to a warehouse files storage")?;
    let built = build(config, root, name, options, log)?;
    for book in &built {
        let paths = destinations(book);
        let latest = latest_text(book, &paths)?;
        let archive = std::fs::read(&book.archive)
            .with_context(|| format!("reading {}", book.archive.display()))?;
        let bodies: [(&str, Vec<u8>); 3] = [
            ("application/gzip", archive),
            ("application/json", book.manifest_text.clone().into_bytes()),
            ("application/json", latest.into_bytes()),
        ];
        if options.dry_run {
            for (path, (_, body)) in paths.iter().zip(&bodies) {
                writeln!(log, "would send {storage}/{path} ({} bytes)", body.len())?;
            }
            continue;
        }
        let token = token()?;
        for (path, (content_type, body)) in paths.iter().zip(bodies) {
            let upload = Upload {
                url: format!(
                    "{warehouse}/api/v1/files/{}/file?path={}",
                    encode_path(storage),
                    encode_path(path)
                ),
                token: token.clone(),
                content_type: content_type.to_string(),
                body,
            };
            let (status, text) = put(&upload)?;
            match status {
                200 | 201 => writeln!(log, "sent {storage}/{path}")?,
                401 | 403 => bail!(
                    "the warehouse refused {storage}/{path} ({status}): `anvil login --scope warehouse:write` (a write grant on the storage is needed)"
                ),
                404 => bail!(
                    "the warehouse has no files storage `{storage}` (404): create it first, or fix `storage` under [docs]"
                ),
                _ => bail!("the warehouse answered {status} for {storage}/{path}: {text}"),
            }
        }
        writeln!(log, "released {} {}", book.book, book.version)?;
    }
    Ok(())
}

/// `anvil docs release`.
///
/// # Errors
/// See [`release_with`].
pub fn release(config: &Config, name: Option<&str>, options: &Options) -> Result<()> {
    let root = std::env::current_dir().context("finding the working directory")?;
    let token_root = root.clone();
    release_with(
        config,
        &root,
        name,
        options,
        &mut std::io::stdout(),
        &move || {
            let bearer =
                crate::commands::login::registry_token(&token_root, |n| std::env::var(n).ok())?;
            Ok(bearer.trim_start_matches("Bearer ").to_string())
        },
        &send,
    )
}
