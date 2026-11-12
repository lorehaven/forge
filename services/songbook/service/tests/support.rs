//! Shared helpers: a scratch directory, a Postgres schema of its own with the Foundry migrations applied, and canned Deezer
//! answers.
//!
//! The tests that need the database run when `SONGBOOK_TEST_DATABASE_URL` points at a Postgres (any database; each test makes
//! a schema of its own and drops it), and skip themselves otherwise, like the other services' integration tests.

use quench_serde::{Value, json};
use songbook_service::app::App;
use songbook_service::config::Config;
use songbook_service::deezer::{Canned, Deezer};
use songbook_service::index::{Index, db};
use songbook_service::jobs::Jobs;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

static COUNTER: AtomicU32 = AtomicU32::new(0);

const MIGRATIONS: [&str; 2] = [
    include_str!("../../../foundry/service/migrations/songbook/0001-library.toml"),
    include_str!("../../../foundry/service/migrations/songbook/0002-listeners.toml"),
];

/// The database the tests use, if one is set.
pub fn database_url() -> Option<String> {
    std::env::var("SONGBOOK_TEST_DATABASE_URL")
        .ok()
        .filter(|url| !url.trim().is_empty())
}

/// Is there a database to test against? Prints why the test is skipped when there is not.
pub fn have_database() -> bool {
    let have = database_url().is_some();
    if !have {
        eprintln!("skipping: SONGBOOK_TEST_DATABASE_URL is not set");
    }
    have
}

/// The SQL statements of the migration files (`sql = "..."` or `sql = """..."""`), `${schema}` replaced.
#[allow(clippy::literal_string_with_formatting_args)]
fn statements(schema: &str) -> Vec<String> {
    let mut out = Vec::new();
    for file in MIGRATIONS {
        let mut lines = file.lines();
        while let Some(line) = lines.next() {
            let Some(rest) = line.strip_prefix("sql = ") else {
                continue;
            };
            let sql = if rest == "\"\"\"" {
                let mut body = String::new();
                for l in lines.by_ref() {
                    if l == "\"\"\"" {
                        break;
                    }
                    body.push_str(l);
                    body.push('\n');
                }
                body
            } else {
                rest.trim_matches('"').to_string()
            };
            out.push(sql.replace("${schema}", schema));
        }
    }
    out
}

/// A directory that is removed when it goes out of scope, and the schema of the test's own tables.
#[derive(Debug)]
pub struct Scratch {
    path: PathBuf,
    schema: String,
    made_schema: AtomicBool,
}

impl Scratch {
    pub fn new() -> Self {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let pid = std::process::id();
        let dir = std::env::temp_dir().join(format!("songbook-test-{pid}-{n}"));
        std::fs::create_dir_all(&dir).unwrap();
        Self {
            path: dir,
            schema: format!("songbook_t{pid}_{n}"),
            made_schema: AtomicBool::new(false),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn schema(&self) -> &str {
        &self.schema
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
        if let (true, Some(url)) = (self.made_schema.load(Ordering::SeqCst), database_url()) {
            let schema = self.schema.clone();
            // Dropped from a thread of its own: this may run inside the test's runtime.
            let _ = std::thread::spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap();
                runtime.block_on(async move {
                    if let Ok(pool) = quench_pg::Pool::connect(&url).await {
                        let _ = pool
                            .batch_execute(&format!("DROP SCHEMA IF EXISTS {schema} CASCADE"))
                            .await;
                    }
                });
            })
            .join();
        }
    }
}

/// The pool for the scratch schema, with the tables made.
pub async fn pool_in(dir: &Scratch) -> quench_pg::Pool {
    let url = database_url().expect("SONGBOOK_TEST_DATABASE_URL");
    let admin = quench_pg::Pool::connect(&url).await.unwrap();
    admin
        .batch_execute(&format!("CREATE SCHEMA IF NOT EXISTS {}", dir.schema()))
        .await
        .unwrap();
    dir.made_schema.store(true, Ordering::SeqCst);
    for sql in statements(dir.schema()) {
        admin.batch_execute(&sql).await.unwrap();
    }
    db::connect(&url, dir.schema()).await.unwrap()
}

pub async fn index_in(dir: &Scratch) -> Index {
    Index::open(pool_in(dir).await).await.unwrap()
}

pub fn parse(text: &str) -> Value {
    json::parse(text).unwrap()
}

/// An app over a scratch home and canned Deezer answers (`path` or `path?index=N` to JSON text).
pub async fn app_with(dir: &Scratch, answers: &[(&str, &str)]) -> App {
    let config = Config {
        home: dir.path().to_path_buf(),
        data: dir.path().join("data"),
        cookies_file: dir.path().join("meta").join("cookies.txt"),
        cookies_browser: String::new(),
        database_url: database_url().unwrap_or_default(),
        schema: dir.schema().to_string(),
    };
    let table: HashMap<String, Value> = answers
        .iter()
        .map(|(k, v)| ((*k).to_string(), parse(v)))
        .collect();
    let deezer = Deezer::new(Arc::new(Canned(table)), config.cache()).instant();
    let pool = pool_in(dir).await;
    App {
        index: Index::open(pool.clone()).await.unwrap(),
        config: Arc::new(config),
        jobs: Jobs::default(),
        deezer,
        library: songbook_service::library::Library::default(),
        playlists: songbook_service::playlists::Playlists::new(pool),
    }
}
