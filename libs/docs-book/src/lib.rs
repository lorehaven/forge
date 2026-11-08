//! A documentation book: a Jekyll site of Markdown pages with just-the-docs navigation, as the estate's books
//! are (`docs/` in forge and quench). What `anvil docs` checks before building one, and what a reader (loupe)
//! needs to browse one without building it: the pages and their front matter, the navigation tree, where a
//! link leads.
//!
//! ```no_run
//! use docs_book::{Book, Target};
//!
//! let book = Book::open("docs".as_ref())?;
//! for (page, depth) in book.nav().walk() {
//!     println!("{}{}", "  ".repeat(depth), book.pages()[page].label());
//! }
//! let metrics = book.page("services/gauge/metrics").unwrap();
//! match book.resolve(metrics, "configuration.md#targets") {
//!     Target::Page { page, anchor } => println!("{} {anchor:?}", book.pages()[page].path),
//!     other => println!("{other:?}"),
//! }
//! # Ok::<(), quench_core::error::Error>(())
//! ```

mod links;
mod nav;
mod page;

pub use links::{Target, is_outside, link_problems, links, resolve};
pub use nav::{Nav, nav_problems};
pub use page::{Page, parse_page, read_pages, split_front_matter, strip_front_matter};

use quench_core::{Result, bail};
use quench_serde::Value;
use std::path::{Path, PathBuf};

/// A book read from its directory: its pages, the problems met reading them, its navigation and settings.
#[derive(Debug, Clone)]
pub struct Book {
    dir: PathBuf,
    title: Option<String>,
    baseurl: String,
    pages: Vec<Page>,
    problems: Vec<String>,
    nav: Nav,
}

impl Book {
    /// Reads the book in `dir`.
    ///
    /// # Errors
    /// `dir` is not a directory, or it, a page or its `_config.yml` cannot be read.
    pub fn open(dir: &Path) -> Result<Self> {
        if !dir.is_dir() {
            bail!("the book's directory {} does not exist", dir.display());
        }
        let config = page::config(dir)?;
        let setting = |key: &str| {
            config
                .as_ref()
                .and_then(|c| c.get(key))
                .and_then(Value::as_str)
                .map(str::to_string)
        };
        let (pages, problems) = read_pages(dir)?;
        let nav = Nav::of(&pages);
        Ok(Self {
            dir: dir.to_path_buf(),
            title: setting("title"),
            baseurl: setting("baseurl").unwrap_or_default(),
            pages,
            problems,
            nav,
        })
    }

    /// The book's directory.
    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// `title` of `_config.yml`.
    #[must_use]
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    /// `baseurl` of `_config.yml` (`/docs`), empty when the site has none.
    #[must_use]
    pub fn baseurl(&self) -> &str {
        &self.baseurl
    }

    /// The pages, in path order.
    #[must_use]
    pub fn pages(&self) -> &[Page] {
        &self.pages
    }

    /// The navigation tree.
    #[must_use]
    pub const fn nav(&self) -> &Nav {
        &self.nav
    }

    /// The page at `route` (`services/gauge/metrics`, `services/gauge` for its `index.md`, the empty string or
    /// `/` for the book's own index); a path with `.md` or `.html`, or a permalink, is accepted too.
    #[must_use]
    pub fn page(&self, route: &str) -> Option<usize> {
        let route = route.trim().trim_matches('/');
        self.pages
            .iter()
            .position(|p| p.route() == route)
            .or_else(
                || match resolve(&self.pages, "", &format!("/{route}"), "") {
                    Target::Page { page, .. } => Some(page),
                    _ => None,
                },
            )
    }

    /// Where `target`, a link on page `from`, leads.
    #[must_use]
    pub fn resolve(&self, from: usize, target: &str) -> Target {
        let path = self.pages.get(from).map_or("", |p| p.path.as_str());
        resolve(&self.pages, path, target, &self.baseurl)
    }

    /// Every problem of the book: pages without front matter or with front matter that is not YAML, the
    /// navigation's problems, and links to nothing.
    #[must_use]
    pub fn problems(&self) -> Vec<String> {
        let mut out = self.problems.clone();
        out.extend(nav_problems(&self.pages));
        for page in &self.pages {
            out.extend(link_problems(&self.dir, page));
        }
        out
    }
}
