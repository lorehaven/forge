//! The database Mailroom needs for the suite.
//!
//! Mailroom keeps its state in Postgres, which the in-memory suites do without. When `MAILROOM_BDD_DATABASE_URL` is set (any
//! database; the suite makes a schema of its own) Mailroom is part of the run, and gatehouse writes its mail **into** it: every
//! verification, reset and invitation the gatehouse scenarios use has crossed Gatehouse -> Mailroom -> a person's mailbox (the
//! links are read back from gatehouse's log, as the logging sender writes them). Without it gatehouse logs its mail as before
//! and the Mailroom scenarios are left out.

use std::path::{Path, PathBuf};

pub const SCHEMA: &str = "bdd_mailroom";
/// The part of every address in the suite.
pub const DOMAIN: &str = "bdd.example";

/// The database the suite may use for Mailroom, if one is configured.
#[must_use]
pub fn database_url() -> Option<String> {
    std::env::var("MAILROOM_BDD_DATABASE_URL")
        .ok()
        .filter(|u| !u.trim().is_empty())
}

/// Whether Mailroom takes part in this run.
#[must_use]
pub fn enabled() -> bool {
    database_url().is_some()
}

/// Makes the schema Mailroom runs in, from the Foundry migrations, dropping what an earlier run left.
///
/// # Panics
///
/// When the database cannot be reached or the migration does not apply.
pub async fn prepare_schema() {
    let url = database_url().expect("MAILROOM_BDD_DATABASE_URL");
    let pool = quench_pg::Pool::connect(&url)
        .await
        .unwrap_or_else(|e| panic!("MAILROOM_BDD_DATABASE_URL: {e}"));
    pool.batch_execute(&format!(
        "DROP SCHEMA IF EXISTS {SCHEMA} CASCADE; CREATE SCHEMA {SCHEMA};"
    ))
    .await
    .expect("make the schema");
    for file in migrations() {
        let migration = std::fs::read_to_string(&file)
            .unwrap_or_else(|e| panic!("read {}: {e}", file.display()));
        for sql in statements(&migration) {
            pool.batch_execute(&sql.replace("${schema}", SCHEMA))
                .await
                .unwrap_or_else(|e| panic!("apply {}: {e}", file.display()));
        }
    }
}

/// Every migration of Mailroom's Foundry module, in the order of their numbers. Read from the directory, so that a new one is
/// in the suite without being named here.
fn migrations() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../services/foundry/service/migrations/mailroom");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .map(|entry| entry.expect("a directory entry").path())
        .filter(|p| {
            p.extension().is_some_and(|e| e == "toml")
                && p.file_name().is_some_and(|n| n != "module.toml")
        })
        .collect();
    files.sort();
    files
}

/// The SQL statements of a Foundry migration file (`sql = "..."` or `sql = """..."""`).
fn statements(file: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut lines = file.lines();
    while let Some(line) = lines.next() {
        let Some(rest) = line.strip_prefix("sql = ") else {
            continue;
        };
        if rest == "\"\"\"" {
            let mut body = String::new();
            for l in lines.by_ref() {
                if l == "\"\"\"" {
                    break;
                }
                body.push_str(l);
                body.push('\n');
            }
            out.push(body);
        } else {
            out.push(rest.trim_matches('"').to_string());
        }
    }
    out
}
