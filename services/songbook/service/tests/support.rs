//! Shared helpers: a scratch directory, an index in it, and canned Deezer answers.

use quench_serde::{Value, json};
use songbook_service::app::App;
use songbook_service::config::Config;
use songbook_service::deezer::{Canned, Deezer};
use songbook_service::index::Index;
use songbook_service::jobs::Jobs;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

static COUNTER: AtomicU32 = AtomicU32::new(0);

/// A directory that is removed when it goes out of scope.
#[derive(Debug)]
pub struct Scratch(pub PathBuf);

impl Scratch {
    pub fn new() -> Self {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("songbook-test-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub fn index_in(dir: &Scratch) -> Index {
    Index::open(
        &dir.path().join("meta").join("library.db"),
        &dir.path().join("meta"),
    )
    .unwrap()
}

pub fn parse(text: &str) -> Value {
    json::parse(text).unwrap()
}

/// An app over a scratch home and canned Deezer answers (`path` or `path?index=N` to JSON text).
pub fn app_with(dir: &Scratch, answers: &[(&str, &str)]) -> App {
    let config = Config {
        home: dir.path().to_path_buf(),
        data: dir.path().join("data"),
        cookies_file: dir.path().join("meta").join("cookies.txt"),
        cookies_browser: String::new(),
    };
    let table: HashMap<String, Value> = answers
        .iter()
        .map(|(k, v)| ((*k).to_string(), parse(v)))
        .collect();
    let deezer = Deezer::new(Arc::new(Canned(table)), config.cache()).instant();
    App {
        index: index_in(dir),
        config: Arc::new(config),
        jobs: Jobs::default(),
        deezer,
    }
}
