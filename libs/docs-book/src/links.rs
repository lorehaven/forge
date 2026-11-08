//! Links in a book: finding them in a page's Markdown, checking that they lead somewhere, and resolving one to
//! the page (and heading) it means, the way the built site would serve it: `metrics.md`, `metrics.html`,
//! `gauge/` (its `index.md`), `/services/gauge/metrics.html` from the site root, or a page's `permalink`.

use crate::page::Page;
use std::path::{Component, Path, PathBuf};

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

/// Whether a link leaves the book (a URL with a scheme, `mailto:`...), or is not a path at all.
#[must_use]
pub fn is_outside(target: &str) -> bool {
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
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir | Component::RootDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// The links of `page` that lead to no file of the book in `dir`.
#[must_use]
pub fn link_problems(dir: &Path, page: &Page) -> Vec<String> {
    let mut out = Vec::new();
    let from = Path::new(&page.path)
        .parent()
        .unwrap_or_else(|| Path::new(""));
    for target in links(&page.body) {
        if is_outside(&target) {
            continue;
        }
        let path = target.split(['#', '?']).next().unwrap_or_default();
        if path.is_empty() {
            continue;
        }
        let joined = path
            .strip_prefix('/')
            .map_or_else(|| from.join(path), PathBuf::from);
        let normal = normalise(&joined);
        let full = dir.join(&normal);
        let found = full.is_file() || (full.is_dir() && full.join("index.md").is_file());
        // Jekyll leaves out what starts with `_` or `.`: such a file is in the repository, not on the site.
        let hidden = normal.components().any(|part| {
            part.as_os_str()
                .to_str()
                .is_some_and(|name| name.starts_with('_') || name.starts_with('.'))
        });
        if !found {
            out.push(format!("{}: broken link `{target}`", page.path));
        } else if hidden {
            out.push(format!(
                "{}: link `{target}` leads to a file Jekyll leaves out of the site (a `_` or `.` path)",
                page.path
            ));
        }
    }
    out
}

/// Where a link leads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// A page of the book (an index into its pages), and the heading it points at.
    Page { page: usize, anchor: Option<String> },
    /// A heading of the page the link is on.
    Anchor(String),
    /// Out of the book: a URL with a scheme, or a path the book does not have a page for (an image, a file).
    Outside(String),
    /// A path in the book that matches no page.
    Missing(String),
}

/// Resolves `target`, written on the page at `from` (its path in the book), among `pages`. `baseurl` is the
/// site's base path (`/docs`), which a site-absolute link may start with; empty when the site has none.
#[must_use]
pub fn resolve(pages: &[Page], from: &str, target: &str, baseurl: &str) -> Target {
    let target = target.trim();
    if let Some(anchor) = target.strip_prefix('#') {
        return Target::Anchor(anchor.to_string());
    }
    if is_outside(target) {
        return Target::Outside(target.to_string());
    }
    let (rest, anchor) = match target.split_once('#') {
        Some((path, anchor)) => (path, Some(anchor.to_string())),
        None => (target, None),
    };
    let path = rest.split('?').next().unwrap_or_default();
    let absolute = path.starts_with('/');
    let base = baseurl.trim_end_matches('/');
    let path = if absolute && !base.is_empty() {
        path.strip_prefix(base).unwrap_or(path)
    } else {
        path
    };
    let dir_like = path.is_empty() || path.ends_with('/');
    let joined = if absolute {
        PathBuf::from(path)
    } else {
        Path::new(from)
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .join(path)
    };
    let normal = normalise(&joined).to_string_lossy().replace('\\', "/");
    match find(pages, &normal, dir_like) {
        Some(page) => Target::Page { page, anchor },
        None if has_extension(&normal) && !is_page_file(&normal) => {
            Target::Outside(target.to_string())
        }
        None => Target::Missing(target.to_string()),
    }
}

/// The extension of the last segment of `path`, lower-cased.
fn extension(path: &str) -> Option<String> {
    let name = path.rsplit('/').next()?;
    Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
}

/// Whether `path` names a page: a Markdown file, or the HTML Jekyll writes for one.
fn is_page_file(path: &str) -> bool {
    matches!(extension(path).as_deref(), Some("md" | "html"))
}

/// `path` without its extension.
fn stem(path: &str) -> &str {
    path.rfind('.')
        .filter(|&dot| !path[dot..].contains('/'))
        .map_or(path, |dot| &path[..dot])
}

fn has_extension(path: &str) -> bool {
    path.rsplit('/')
        .next()
        .is_some_and(|name| name.contains('.'))
}

/// The page served at `path` (normalised, no leading `/`): by permalink, as a Markdown file, as the `.html`
/// Jekyll writes for it, or as a directory's `index.md`.
fn find(pages: &[Page], path: &str, dir_like: bool) -> Option<usize> {
    let by_path = |wanted: &str| pages.iter().position(|p| p.path == wanted);
    let trimmed = path.trim_matches('/');
    if let Some(i) = pages.iter().position(|p| {
        p.permalink
            .as_deref()
            .is_some_and(|link| link.trim_matches('/') == trimmed)
    }) {
        return Some(i);
    }
    let index = |dir: &str| {
        if dir.is_empty() {
            by_path("index.md")
        } else {
            by_path(&format!("{dir}/index.md"))
        }
    };
    if dir_like {
        return index(trimmed);
    }
    match extension(path).as_deref() {
        Some("md") => return by_path(&format!("{}.md", stem(path))),
        Some("html") => {
            let stem = stem(path);
            return if stem == "index" || stem.ends_with("/index") {
                by_path(&format!("{stem}.md"))
            } else {
                by_path(&format!("{stem}.md")).or_else(|| index(stem))
            };
        }
        _ => {}
    }
    if has_extension(path) {
        return None;
    }
    by_path(&format!("{path}.md")).or_else(|| index(path))
}
