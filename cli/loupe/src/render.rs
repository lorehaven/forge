//! A page's Markdown to lines: front matter stripped, `{{ site.baseurl }}` replaced, parsed with every
//! extension, laid out with formulas as Unicode text through quench-tex.

use docs_book::strip_front_matter;
use quench_html::markdown::{self, Options as Syntax};
use quench_markdown_term::{
    FootnotePlacement, MathMode, MathText, Options, Rendered, TexMath, render_with_math,
    replace_baseurl,
};
use std::collections::HashSet;
use std::hash::BuildHasher;

/// How a page is laid out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub width: usize,
    pub footnotes: FootnotePlacement,
    pub math: MathMode,
    /// A ` [n]` after links and the list of destinations at the end: for plain output, which has no hyperlinks.
    pub numbered_links: bool,
    /// What `{{ site.baseurl }}` stands for.
    pub baseurl: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            width: 80,
            footnotes: FootnotePlacement::End,
            math: MathMode::Text,
            numbered_links: true,
            baseurl: String::new(),
        }
    }
}

/// The math hook of a page: quench-tex's text, except for the formulas switched to their source (`m`).
#[derive(Debug)]
struct PageMath<'a, S> {
    tex: TexMath,
    source: &'a HashSet<String, S>,
}

impl<S: BuildHasher> MathText for PageMath<'_, S> {
    fn inline(&self, tex: &str) -> Option<String> {
        if self.source.contains(tex.trim()) {
            return None;
        }
        self.tex.inline(tex)
    }

    fn display(&self, tex: &str) -> Option<Vec<String>> {
        if self.source.contains(tex.trim()) {
            return None;
        }
        self.tex.display(tex)
    }
}

/// Lays out `markdown` (a whole file: front matter is stripped). `source` holds the formulas (their trimmed
/// TeX) to show as source instead of text.
#[must_use]
pub fn render_page<S: BuildHasher>(
    markdown: &str,
    settings: &Settings,
    source: &HashSet<String, S>,
) -> Rendered {
    let body = replace_baseurl(strip_front_matter(markdown), &settings.baseurl);
    let document = markdown::parse(&body, Syntax::ALL);
    let options = Options {
        width: settings.width.max(4),
        unicode: true,
        footnotes: settings.footnotes,
        math: settings.math,
        baseurl: settings.baseurl.clone(),
        numbered_links: settings.numbered_links,
    };
    let math = PageMath {
        tex: TexMath::new(),
        source,
    };
    render_with_math(&document, &options, &math)
}
