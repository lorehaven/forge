//! The books being served, and keeping them current.

use crate::book::Book;
use crate::source::{Source, SourceError};
use quench_serde::Value;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

/// What one look at the source changed.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Refreshed {
    /// `book version` of each book newly loaded or updated.
    pub loaded: Vec<String>,
    /// Books no longer in the storage.
    pub removed: Vec<String>,
    /// `book: reason` for each book that could not be updated; the copy already held stays in service.
    pub failed: Vec<String>,
}

#[derive(Debug)]
pub struct Library {
    base_path: String,
    books: RwLock<BTreeMap<String, Arc<Book>>>,
    /// Set once the source has been listed successfully, whether or not it held anything.
    listed: AtomicBool,
}

impl Library {
    /// `base_path` is where the service is mounted; each book is served at `<base_path>/<name>/`.
    #[must_use]
    pub fn new(base_path: &str) -> Self {
        Self {
            base_path: base_path.trim_end_matches('/').to_string(),
            books: RwLock::new(BTreeMap::new()),
            listed: AtomicBool::new(false),
        }
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<Arc<Book>> {
        self.books
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(name)
            .cloned()
    }

    /// Every book, by name.
    #[must_use]
    pub fn list(&self) -> Vec<Arc<Book>> {
        self.books
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .values()
            .cloned()
            .collect()
    }

    /// Whether the source has answered at least once.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.listed.load(Ordering::Relaxed)
    }

    pub fn insert(&self, book: Book) {
        self.books
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(book.name.clone(), Arc::new(book));
    }

    /// Looks at the source and brings the library in line with it: loads a book that is new or has a newer
    /// release, drops one that is gone. A book that cannot be updated is reported and left as it was.
    ///
    /// # Errors
    ///
    /// When the source cannot be listed; nothing is changed then.
    pub async fn refresh(&self, source: &dyn Source) -> Result<Refreshed, SourceError> {
        let names = source.books().await?;
        self.listed.store(true, Ordering::Relaxed);
        let mut report = Refreshed::default();

        for name in &names {
            match self.update(source, name).await {
                Ok(Some(version)) => report.loaded.push(format!("{name} {version}")),
                Ok(None) => {}
                Err(reason) => report.failed.push(format!("{name}: {reason}")),
            }
        }

        let gone: Vec<String> = self
            .list()
            .iter()
            .map(|book| book.name.clone())
            .filter(|name| !names.contains(name))
            .collect();
        if !gone.is_empty() {
            {
                let mut books = self
                    .books
                    .write()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                for name in &gone {
                    books.remove(name);
                }
            }
            report.removed = gone;
        }
        Ok(report)
    }

    /// `Some(version)` when `name` was loaded, `None` when it is current already or holds no release.
    async fn update(&self, source: &dyn Source, name: &str) -> Result<Option<String>, String> {
        if crate::ui::RESERVED.contains(&name) {
            return Err(format!(
                "`{name}` is a path of the service itself, not a name for a book"
            ));
        }
        let Some(raw) = source
            .read(&format!("{name}/latest.json"))
            .await
            .map_err(|e| e.to_string())?
        else {
            return Ok(None);
        };
        let latest: Value =
            quench_serde::json::from_slice(&raw).map_err(|e| format!("latest.json: {e}"))?;
        let version = latest
            .get("version")
            .and_then(Value::as_str)
            .ok_or("latest.json names no version")?;
        if self.get(name).is_some_and(|book| book.version == version) {
            return Ok(None);
        }

        let archive_path = latest
            .get("archive_path")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| {
                latest
                    .get("archive")
                    .and_then(Value::as_str)
                    .map(|archive| format!("{name}/{version}/{archive}"))
            })
            .ok_or("latest.json names no archive")?;
        let archive = source
            .read(&archive_path)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("{archive_path} is not in the storage"))?;

        let prefix = format!("{}/{name}", self.base_path);
        let book = Book::from_release(name, &latest, &archive, &prefix)?;
        let loaded = book.version.clone();
        self.insert(book);
        Ok(Some(loaded))
    }
}
