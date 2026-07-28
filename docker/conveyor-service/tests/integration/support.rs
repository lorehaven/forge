//! Shared setup for the tests that need a real Postgres.
//!
//! They are skipped unless `CONVEYOR_TEST_DATABASE_URL` is set, the way the
//! estate skips its Redis-backed cache tests. Point it at a throwaway database:
//! every test truncates conveyor's tables before it runs.
//!
//! ```bash
//! docker run --rm -d --name conveyor-test-pg -e POSTGRES_PASSWORD=postgres \
//!     -p 55432:5432 pgvector/pgvector:pg18
//! cargo run -p foundry-service -- apply \
//!     --catalog docker/foundry-service/migrations \
//!     --database-url postgres://postgres:postgres@localhost:55432/postgres \
//!     --install conveyor
//! CONVEYOR_TEST_DATABASE_URL=postgres://postgres:postgres@localhost:55432/postgres \
//!     cargo test -p conveyor-service
//! ```

#![allow(dead_code)]

use conveyor_service::domain::{Provider, Repo};
use conveyor_service::scheduler::repos::{self, NewRepo};
use quench_db::prelude::{Database, Db};
use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;
use tokio::sync::{Mutex, MutexGuard};

/// The tests share one schema, so they take turns. Running them in parallel
/// would have them claiming each other's runs - `claim_next` deliberately does
/// not know which test queued what.
fn lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

pub const TEST_USER: &str = "conveyor-tests";

/// A connected database with conveyor's tables emptied, or `None` when no test
/// database is configured.
pub async fn database() -> Option<(Db, MutexGuard<'static, ()>)> {
    let url = std::env::var("CONVEYOR_TEST_DATABASE_URL").ok()?;
    if url.trim().is_empty() {
        return None;
    }

    let guard = lock().lock().await;
    let db = Db::connect(&url)
        .await
        .expect("connect to the test database");

    // Cascades to runs, jobs, steps, logs and artifacts.
    db.execute("TRUNCATE conveyor.repos, conveyor.runs CASCADE")
        .await
        .expect("truncate");
    db.execute(&format!(
        "INSERT INTO auth.users (username, password, roles) \
         VALUES ('{TEST_USER}', 'x', '[]'::jsonb) ON CONFLICT DO NOTHING"
    ))
    .await
    .expect("seed the test user");

    Some((db, guard))
}

/// Printed once when the database tests are skipped, so a green run that tested
/// nothing does not look like a green run that tested everything.
pub fn skipped(test: &str) {
    println!("skipping {test}: CONVEYOR_TEST_DATABASE_URL is not set");
}

pub async fn register_repo(db: &Db, name: &str, clone_url: &str) -> Repo {
    repos::create(
        db,
        &NewRepo {
            provider: Provider::Generic,
            owner: "tests".to_string(),
            name: name.to_string(),
            clone_url: clone_url.to_string(),
            default_branch: "master".to_string(),
            registered_by: TEST_USER.to_string(),
        },
    )
    .await
    .expect("register the repository")
}

// ---------------------------------------------------------------------------
// A repository to build
// ---------------------------------------------------------------------------

pub fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "conveyor tests")
        .env("GIT_AUTHOR_EMAIL", "tests@example.invalid")
        .env("GIT_COMMITTER_NAME", "conveyor tests")
        .env("GIT_COMMITTER_EMAIL", "tests@example.invalid")
        .output()
        .unwrap_or_else(|e| panic!("git {args:?}: {e}"));

    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// A repository carrying the given `.conveyor.toml`.
pub struct Origin {
    pub dir: tempfile::TempDir,
    pub sha: String,
}

impl Origin {
    pub fn with_pipeline(pipeline: &str) -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        git(dir.path(), &["init", "--quiet", "-b", "master"]);
        std::fs::write(dir.path().join(".conveyor.toml"), pipeline).expect("write pipeline");
        git(dir.path(), &["add", "-A"]);
        git(dir.path(), &["commit", "--quiet", "-m", "pipeline"]);
        let sha = git(dir.path(), &["rev-parse", "HEAD"]);
        Self { dir, sha }
    }

    /// A repository with no pipeline at all.
    pub fn bare() -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        git(dir.path(), &["init", "--quiet", "-b", "master"]);
        std::fs::write(dir.path().join("README.md"), b"nothing here\n").expect("write");
        git(dir.path(), &["add", "-A"]);
        git(dir.path(), &["commit", "--quiet", "-m", "no pipeline"]);
        let sha = git(dir.path(), &["rev-parse", "HEAD"]);
        Self { dir, sha }
    }

    pub fn url(&self) -> String {
        format!("file://{}", self.dir.path().display())
    }
}
