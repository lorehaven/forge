//! The pages of a book: every Markdown file with front matter, read from its directory the way Jekyll would
//! (files and directories starting with `_` or `.`, and what `_config.yml` excludes, are not part of the site).

use quench_core::{Context, Result};
use quench_serde::Value;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// A Markdown page of a book: where it is, what its front matter says, and its body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    /// From the book's directory, with `/`: `services/gauge/metrics.md`.
    pub path: String,
    pub title: Option<String>,
    pub parent: Option<String>,
    pub grand_parent: Option<String>,
    pub nav_order: Option<i64>,
    pub has_children: bool,
    /// `nav_exclude: true`: a page that is not in the navigation (it can still be linked to).
    pub nav_exclude: bool,
    /// `permalink`, as written: `/crates/quench-tex/`.
    pub permalink: Option<String>,
    /// The Markdown after the front matter.
    pub body: String,
}

impl Page {
    /// The page's route: its path without `.md`, and a directory for an `index.md` (`services/gauge/metrics`,
    /// `services/gauge` for `services/gauge/index.md`, the empty string for the book's own `index.md`).
    #[must_use]
    pub fn route(&self) -> &str {
        let route = self.path.strip_suffix(".md").unwrap_or(&self.path);
        if route == "index" {
            ""
        } else {
            route.strip_suffix("/index").unwrap_or(route)
        }
    }

    /// The title, or the file name when the front matter has none.
    #[must_use]
    pub fn label(&self) -> &str {
        self.title.as_deref().unwrap_or_else(|| {
            let name = self.path.rsplit('/').next().unwrap_or(&self.path);
            name.strip_suffix(".md").unwrap_or(name)
        })
    }
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

/// `text` without its front matter, if it has any: what a page's Markdown is to a reader.
#[must_use]
pub fn strip_front_matter(text: &str) -> &str {
    split_front_matter(text).map_or(text, |(_, body)| body)
}

/// The book's `_config.yml`, if it has one.
pub fn config(dir: &Path) -> Result<Option<Value>> {
    let file = dir.join("_config.yml");
    if !file.is_file() {
        return Ok(None);
    }
    let text =
        std::fs::read_to_string(&file).with_context(|| format!("reading {}", file.display()))?;
    let value: Value = quench_serde::yaml::from_str(&text)
        .with_context(|| format!("{} is not valid YAML", file.display()))?;
    Ok(Some(value))
}

fn exclusions(config: Option<&Value>) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    if let Some(Value::Array(list)) = config.and_then(|c| c.get("exclude")) {
        for item in list {
            if let Some(s) = item.as_str() {
                out.insert(s.trim_end_matches('/').to_string());
            }
        }
    }
    out
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
        let rel = relative(&path, base);
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

fn relative(path: &Path, base: &Path) -> String {
    path.strip_prefix(base)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Every Markdown page of the book in `dir`, and the problems found while reading them (no front matter,
/// front matter that is not YAML).
///
/// # Errors
/// The directory, a page or `_config.yml` cannot be read, or `_config.yml` is not valid YAML.
pub fn read_pages(dir: &Path) -> Result<(Vec<Page>, Vec<String>)> {
    let excluded = exclusions(config(dir)?.as_ref());
    let mut files = Vec::new();
    walk(dir, dir, &excluded, &mut files)?;
    let mut pages = Vec::new();
    let mut problems = Vec::new();
    for file in files {
        if file.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let rel = relative(&file, dir);
        let text = std::fs::read_to_string(&file)
            .with_context(|| format!("reading {}", file.display()))?;
        match parse_page(&rel, &text) {
            Ok(page) => pages.push(page),
            Err(problem) => problems.push(problem),
        }
    }
    Ok((pages, problems))
}

/// A page from its path and text, or the problem that keeps it from being one.
///
/// # Errors
/// The text has no front matter (Jekyll copies such a file as it is), or it is not valid YAML.
pub fn parse_page(path: &str, text: &str) -> std::result::Result<Page, String> {
    let Some((head, body)) = split_front_matter(text) else {
        return Err(format!(
            "{path}: no front matter (Jekyll would not render it as a page)"
        ));
    };
    let value: Value = quench_serde::yaml::from_str(head)
        .map_err(|e| format!("{path}: the front matter is not valid YAML: {e}"))?;
    let text_of = |key: &str| value.get(key).and_then(Value::as_str).map(str::to_string);
    let flag = |key: &str| matches!(value.get(key), Some(Value::Bool(true)));
    Ok(Page {
        path: path.to_string(),
        title: text_of("title"),
        parent: text_of("parent"),
        grand_parent: text_of("grand_parent"),
        nav_order: value.get("nav_order").and_then(Value::as_i64),
        has_children: flag("has_children"),
        nav_exclude: flag("nav_exclude"),
        permalink: text_of("permalink"),
        body: body.to_string(),
    })
}
