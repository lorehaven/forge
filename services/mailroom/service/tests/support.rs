//! Shared helpers: a Postgres schema of its own with the Foundry migrations applied, fake mail servers, and an `App` over them.
//!
//! The tests that need the database run when `MAILROOM_TEST_DATABASE_URL` points at a Postgres (any database; each test makes a
//! schema of its own and drops it) and skip themselves otherwise.

#![allow(dead_code)]

use mail_testkit::{CERT_PEM, ImapOptions, ImapServer, SmtpOptions, SmtpServer};
use mailroom_core::AccountInput;
use mailroom_service::app::App;
use mailroom_service::boot;
use mailroom_service::config::Config;
use mailroom_service::settings::{self, StoredTransport};
use std::sync::atomic::{AtomicU32, Ordering};

static COUNTER: AtomicU32 = AtomicU32::new(0);

const MIGRATIONS: [&str; 1] = [include_str!(
    "../../../foundry/service/migrations/mailroom/0001-mail.toml"
)];

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

/// Everything a test needs: the database, a mail server to read from, one to send through, and the app.
pub struct Env {
    pub app: App,
    pub imap: ImapServer,
    pub smtp: SmtpServer,
    _scratch: Scratch,
}

pub fn raw(subject: &str, from: &str, body: &str) -> Vec<u8> {
    format!("From: {from}\r\nTo: me@example.com\r\nSubject: {subject}\r\nDate: Tue, 29 Sep 2026 12:00:00 +0000\r\nMessage-ID: <{subject}@x.example>\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n{body}\r\n").into_bytes()
}

pub const OWNER: &str = "ann";
pub const PASSWORD: &str = "s3cret-app-password";

pub async fn env() -> Env {
    env_with(|_| {}).await
}

pub async fn env_with(customize: impl FnOnce(&mut Config)) -> Env {
    let scratch = scratch().await;
    let imap = ImapServer::start(ImapOptions::default()).await;
    imap.add_account("me@example.com", PASSWORD);
    imap.add_folder("me@example.com", "Sent", &["\\Sent"]);
    imap.add_folder("me@example.com", "Trash", &["\\Trash"]);
    imap.add_folder("me@example.com", "Archive", &["\\Archive"]);
    let smtp = SmtpServer::start(SmtpOptions::default()).await;
    smtp.add_user("me@example.com", PASSWORD);
    smtp.add_user("noreply@example.com", "relay-pw");
    let mut config = Config::for_tests(&scratch.schema, &database_url().unwrap());
    config.private_hosts = vec!["localhost".into()];
    config.extra_ports = vec![imap.port(), smtp.port()];
    customize(&mut config);
    let app = boot::app(config).await.unwrap();
    Env {
        app,
        imap,
        smtp,
        _scratch: scratch,
    }
}

impl Env {
    /// The form for the fake servers.
    pub fn input(&self) -> AccountInput {
        AccountInput {
            label: "Test".into(),
            address: "me@example.com".into(),
            username: "me@example.com".into(),
            password: PASSWORD.into(),
            imap_host: "localhost".into(),
            imap_port: self.imap.port(),
            imap_security: "tls".into(),
            smtp_host: "localhost".into(),
            smtp_port: self.smtp.port(),
            smtp_security: "tls".into(),
            signature: String::new(),
            tls_roots_pem: CERT_PEM.to_string(),
            save_sent: true,
        }
    }

    /// Adds the mailbox for `owner` and syncs it.
    pub async fn add_account(&self, owner: &str) -> mailroom_core::Account {
        let account = mailroom_service::accounts::create(&self.app, owner, &self.input())
            .await
            .unwrap();
        mailroom_service::sync::sync_account(&self.app, &account.id)
            .await
            .unwrap();
        account
    }

    pub fn deliver(&self, folder: &str, subject: &str, body: &str) -> u32 {
        self.imap.deliver(
            "me@example.com",
            folder,
            &raw(subject, "Alice <alice@x.example>", body),
            &[],
        )
    }

    /// Gives the estate a sending server (the SMTP fake) and a sender address.
    pub async fn with_transport(&self) {
        let mut stored = self.app.settings();
        stored.transport = Some(StoredTransport {
            host: "localhost".into(),
            port: self.smtp.port(),
            security: "tls".into(),
            username: "noreply@example.com".into(),
            tls_roots_pem: CERT_PEM.to_string(),
        });
        stored.identities = vec![mailroom_core::admin::Identity {
            address: "noreply@example.com".into(),
            name: "Forge".into(),
            default: true,
        }];
        settings::save(&self.app.store, &stored).await.unwrap();
        settings::save_transport_secret(&self.app.store, &self.app.vault, "relay-pw")
            .await
            .unwrap();
        self.app.set_settings(stored);
    }

    /// The folder of the mailbox with this role.
    pub async fn folder(&self, account_id: &str, role: &str) -> mailroom_core::Folder {
        mailroom_service::mailbox::folders(&self.app, OWNER, account_id)
            .await
            .unwrap()
            .into_iter()
            .find(|f| f.role == role)
            .unwrap_or_else(|| panic!("no {role} folder"))
    }
}
