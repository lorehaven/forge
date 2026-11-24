//! Shared helpers: a Postgres schema of its own with the Foundry migrations applied, and an `App` over it.
//!
//! The tests that need the database run when `MAILROOM_TEST_DATABASE_URL` points at a Postgres (any database; each test makes a
//! schema of its own and drops it) and skip themselves otherwise.

#![allow(dead_code)]

use mailroom_service::app::App;
use mailroom_service::boot;
use mailroom_service::config::Config;
use std::sync::atomic::{AtomicU32, Ordering};

static COUNTER: AtomicU32 = AtomicU32::new(0);

const MIGRATIONS: [&str; 2] = [
    include_str!("../../../foundry/service/migrations/mailroom/0001-mail.toml"),
    include_str!("../../../foundry/service/migrations/mailroom/0002-drafts-and-mutes.toml"),
];

pub fn database_url() -> Option<String> {
    std::env::var("MAILROOM_TEST_DATABASE_URL")
        .ok()
        .filter(|u| !u.trim().is_empty())
}

/// Whether there is a database to test against; says so when there is not.
pub fn have_database() -> bool {
    let have = database_url().is_some();
    if !have {
        eprintln!("skipping: MAILROOM_TEST_DATABASE_URL is not set");
    }
    have
}

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

/// A schema that is dropped when the value goes out of scope.
#[derive(Debug)]
pub struct Scratch {
    schema: String,
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let (Some(url), schema) = (database_url(), self.schema.clone()) else {
            return;
        };
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

pub async fn scratch() -> Scratch {
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let schema = format!("mailroom_t{}_{n}", std::process::id());
    let url = database_url().expect("MAILROOM_TEST_DATABASE_URL");
    let admin = quench_pg::Pool::connect(&url).await.unwrap();
    admin
        .batch_execute(&format!("CREATE SCHEMA IF NOT EXISTS {schema}"))
        .await
        .unwrap();
    let scratch = Scratch {
        schema: schema.clone(),
    };
    for sql in statements(&schema) {
        admin.batch_execute(&sql).await.unwrap();
    }
    scratch
}

/// Everything a test needs: the database and the app over it.
pub struct Env {
    pub app: App,
    _scratch: Scratch,
}

pub async fn env() -> Env {
    env_with(|_| {}).await
}

pub async fn env_with(customize: impl FnOnce(&mut Config)) -> Env {
    let scratch = scratch().await;
    let mut config = Config::for_tests(&scratch.schema, &database_url().unwrap());
    customize(&mut config);
    let app = boot::app(config).await.unwrap();
    Env {
        app,
        _scratch: scratch,
    }
}

impl Env {
    /// Opens (makes) the mailbox of each name.
    pub async fn people(&self, names: &[&str]) {
        for name in names {
            mailroom_service::mailbox::open_mailbox(&self.app, name)
                .await
                .unwrap();
        }
    }

    /// `from` writes to `to` (names), as a person.
    pub async fn write(
        &self,
        from: &str,
        to: &[&str],
        subject: &str,
        text: &str,
    ) -> mailroom_core::Sent {
        mailroom_service::outgoing::send_user(
            &self.app,
            from,
            &mailroom_core::Compose {
                to: to.iter().map(|t| (*t).to_string()).collect(),
                subject: subject.into(),
                text: text.into(),
                ..mailroom_core::Compose::default()
            },
        )
        .await
        .unwrap()
    }

    /// A service writes to people.
    pub async fn service_mail(
        &self,
        service: &str,
        to: &[&str],
        subject: &str,
        text: &str,
    ) -> mailroom_service::error::Result<mailroom_core::Sent> {
        mailroom_service::outgoing::send_service(
            &self.app,
            service,
            &mailroom_core::SendRequest {
                to: to.iter().map(|t| (*t).to_string()).collect(),
                subject: subject.into(),
                text: text.into(),
                ..mailroom_core::SendRequest::default()
            },
        )
        .await
    }

    /// The messages in a folder of `owner`.
    pub async fn folder(&self, owner: &str, folder: &str) -> Vec<mailroom_core::MessageSummary> {
        mailroom_service::mailbox::list(
            &self.app,
            owner,
            &mailroom_service::mailbox::ListSpec {
                folder: folder.into(),
                limit: 100,
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .items
    }
}
