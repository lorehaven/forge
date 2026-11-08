//! Helpers: scratch directories, a pager over a page, keys typed in a row.

use loupe::pager::{Effect, Key, Pager};
use loupe::render::Settings;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

/// A fresh, empty directory under the system's temp directory, removed when dropped.
#[derive(Debug)]
pub struct Scratch(pub PathBuf);

impl Scratch {
    pub fn new(name: &str) -> Self {
        static COUNT: AtomicUsize = AtomicUsize::new(0);
        let n = COUNT.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("loupe-test-{}-{name}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    /// Writes `text` to `name` (directories created as needed) and returns its path.
    pub fn write(&self, name: &str, text: &str) -> PathBuf {
        let path = self.0.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, text).unwrap();
        path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Settings for the pager: no link numbers, width set by the pager.
pub fn settings() -> Settings {
    Settings {
        numbered_links: false,
        ..Settings::default()
    }
}

/// A pager over `markdown` on a `columns` × `rows` screen.
pub fn pager(markdown: &str, columns: usize, rows: usize) -> Pager {
    Pager::file(
        "page.md",
        None,
        markdown.to_string(),
        settings(),
        None,
        (columns, rows),
    )
}

/// Presses each key; returns the last effect.
pub fn press(pager: &mut Pager, keys: &[Key]) -> Option<Effect> {
    let mut last = None;
    for &key in keys {
        last = pager.key(key);
    }
    last
}

/// Types `text` one character at a time.
pub fn typed(text: &str) -> Vec<Key> {
    text.chars().map(Key::Char).collect()
}

/// The screen's content rows' text (without the status line).
pub fn rows(pager: &Pager) -> Vec<String> {
    pager
        .screen()
        .rows
        .iter()
        .map(|r| r.text().trim_end().to_string())
        .collect()
}

/// A long page: a title, three sections of numbered lines, links, a footnote, a formula and a code block.
pub fn long_page() -> String {
    let mut page = String::from(
        "# Title\n\nSee [the second section](#second) and [a site](https://example.com).\n\n",
    );
    page.push_str("A note[^n] here.\n\n");
    page.push_str("## First\n\n");
    for i in 0..30 {
        let _ = write!(page, "first line {i}\n\n");
    }
    page.push_str(
        "## Second\n\nThe needle is here, and $x^2$ too.\n\n```sh\necho one\necho two\n```\n\n",
    );
    for i in 0..30 {
        let _ = write!(page, "second line {i}\n\n");
    }
    page.push_str("## Third\n\nAnother needle.\n\n[^n]: The footnote's text.\n");
    page
}

/// No network: every request fails, and there are no credentials.
#[derive(Debug)]
pub struct NoNet;

impl loupe::released::Net for NoNet {
    fn get(&self, url: &str, _token: Option<&str>) -> Result<(u16, Vec<u8>), String> {
        panic!("no network in this test: {url}")
    }

    fn token(&self) -> Result<Option<String>, String> {
        Ok(None)
    }
}
