//! Shared setup for the tests that need a real Postgres.
//!
//! Skipped unless `WORKBENCH_TEST_DATABASE_URL` is set, the way conveyor's own
//! Postgres-backed tests skip without `CONVEYOR_TEST_DATABASE_URL`. Point it at
//! a throwaway database - every test truncates workbench's tables before it
//! runs.
//!
//! ```bash
//! docker run --rm -d --name workbench-test-pg -e POSTGRES_PASSWORD=postgres \
//!     -p 55432:5432 pgvector/pgvector:pg18
//! cargo run -p foundry-service -- apply \
//!     --catalog docker/foundry-service/migrations \
//!     --database-url postgres://postgres:postgres@localhost:55432/postgres \
//!     --install workbench
//! WORKBENCH_TEST_DATABASE_URL=postgres://postgres:postgres@localhost:55432/postgres \
//!     cargo test -p workbench-service
//! ```

#![allow(dead_code)]

use quench_db::prelude::{Database, Db};
use std::sync::OnceLock;
use tokio::sync::{Mutex, MutexGuard};
use workbench_service::domain::project::{self, NewProject};

/// The tests share one schema, so they take turns rather than racing each
/// other's rows under a shared truncate.
fn lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

pub const TEST_USER: &str = "workbench-tests";

/// A connected database with workbench's tables emptied, or `None` when no
/// test database is configured.
pub async fn database() -> Option<(Db, MutexGuard<'static, ()>)> {
    let url = std::env::var("WORKBENCH_TEST_DATABASE_URL").ok()?;
    if url.trim().is_empty() {
        return None;
    }

    let guard = lock().lock().await;
    let db = Db::connect(&url)
        .await
        .expect("connect to the test database");

    // Cascades to issues, comments, labels and issue_labels - projects is the
    // one table everything else eventually references.
    db.execute("TRUNCATE workbench.projects CASCADE")
        .await
        .expect("truncate");
    db.execute(&format!(
        "INSERT INTO auth.users (username, password, roles) \
         VALUES ('{TEST_USER}', 'x', '[]'::jsonb) ON CONFLICT DO NOTHING"
    ))
    .await
    .expect("seed the test user");

    // `routers::ui::common::actor`'s auth bypass (`JwtConfig::for_tests()`)
    // synthesizes an all-access identity literally named "admin" - any test
    // that goes through an HTTP handler (rather than calling `domain::*`
    // directly) attributes writes to that username, so it needs a real
    // `auth.users` row too or a foreign key on `reporter`/`assignee`/etc.
    // rejects it.
    db.execute(
        "INSERT INTO auth.users (username, password, roles) \
         VALUES ('admin', 'x', '[]'::jsonb) ON CONFLICT DO NOTHING",
    )
    .await
    .expect("seed the admin user");

    Some((db, guard))
}

/// Printed once when the database tests are skipped, so a green run that
/// tested nothing does not look like a green run that tested everything.
pub fn skipped(test: &str) {
    println!("skipping {test}: WORKBENCH_TEST_DATABASE_URL is not set");
}

pub async fn new_project(db: &Db, key: &str, name: &str) -> project::Project {
    project::create(
        db,
        &NewProject {
            key: key.to_string(),
            name: name.to_string(),
            description: None,
        },
    )
    .await
    .expect("create the project")
}
