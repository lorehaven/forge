//! The mail side of the suite: a fake sending server the estate's mail ends up at, a fake provider mailbox a person reads from,
//! the database Mailroom needs, and a tap that turns what the sending server received into the log lines gatehouse's own
//! mail steps read.
//!
//! Mailroom keeps its state in Postgres, which the in-memory suites do without. When `MAILROOM_BDD_DATABASE_URL` is set (any
//! database; the suite makes a schema of its own) Mailroom is part of the run, and gatehouse sends **through** it: every
//! verification, reset and invitation mail the gatehouse scenarios use has crossed Gatehouse -> Mailroom -> SMTP. Without it
//! gatehouse logs its mail as before and the Mailroom scenarios are left out.

use mail_testkit::{CERT_PEM, ImapOptions, ImapServer, Received, SmtpOptions, SmtpServer};
use std::sync::{Arc, Mutex, OnceLock};
use tokio::time::{Duration, sleep};

/// Where the fakes listen. Outside the mail ports on purpose: Mailroom is told to allow them, as an administrator would.
pub const SMTP_PORT: u16 = 12525;
pub const IMAP_PORT: u16 = 12993;
pub const SCHEMA: &str = "bdd_mailroom";
/// The estate's relay login the fake sending server accepts.
pub const RELAY_USER: &str = "noreply@bdd.example";
pub const RELAY_PASSWORD: &str = "bdd-relay-password";
/// The person's mailbox at the fake provider.
pub const MAILBOX_USER: &str = "me@provider.example";
pub const MAILBOX_PASSWORD: &str = "bdd-mailbox-app-password";

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

/// The fakes, alive for the whole run.
pub struct Infra {
    pub smtp: SmtpServer,
    pub imap: ImapServer,
}

static INFRA: OnceLock<Arc<Infra>> = OnceLock::new();

/// The running fakes.
///
/// # Panics
///
/// When [`start`] has not run.
#[must_use]
pub fn infra() -> Arc<Infra> {
    INFRA.get().expect("mail fakes not started").clone()
}

/// Starts the fakes and the tap. The ports are fixed because Mailroom is told about them in its environment.
pub async fn start() {
    if INFRA.get().is_some() {
        return;
    }
    let smtp = SmtpServer::start_on(SMTP_PORT, SmtpOptions::default()).await;
    smtp.add_user(RELAY_USER, RELAY_PASSWORD);
    // A person's own mailbox sends through the same fake server, logging in as themselves.
    smtp.add_user(MAILBOX_USER, MAILBOX_PASSWORD);
    let imap = ImapServer::start_on(IMAP_PORT, ImapOptions::default()).await;
    imap.add_account(MAILBOX_USER, MAILBOX_PASSWORD);
    imap.add_folder(MAILBOX_USER, "Sent", &["\\Sent"]);
    imap.add_folder(MAILBOX_USER, "Trash", &["\\Trash"]);
    let infra = Arc::new(Infra { smtp, imap });
    let _ = INFRA.set(infra.clone());
    tokio::spawn(tap(infra));
}

/// The certificate the fakes present.
#[must_use]
pub const fn certificate() -> &'static str {
    CERT_PEM
}

/// Makes the schema Mailroom runs in, from the Foundry migration, dropping what an earlier run left.
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
    let migration =
        include_str!("../../../services/foundry/service/migrations/mailroom/0001-mail.toml");
    for sql in statements(migration) {
        pool.batch_execute(&sql.replace("${schema}", SCHEMA))
            .await
            .expect("apply the migration");
    }
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

/// What the sending server has received, as the harness shares it with the steps.
#[derive(Default)]
pub struct Inbox {
    seen: Mutex<usize>,
}

static INBOX: OnceLock<Inbox> = OnceLock::new();

/// Everything the estate's sending server has received so far.
#[must_use]
pub fn received() -> Vec<Received> {
    infra().smtp.received()
}

/// Waits for a message to `address` (and, when given, of this `X-Forge-Mail` kind) and returns the newest.
pub async fn wait_for(address: &str, kind: Option<&str>) -> Option<Received> {
    for _ in 0..100 {
        let found = received().into_iter().rev().find(|m| {
            m.to.iter().any(|t| t.eq_ignore_ascii_case(address))
                && kind.is_none_or(|k| m.header("x-forge-mail").as_deref() == Some(k))
        });
        if found.is_some() {
            return found;
        }
        sleep(Duration::from_millis(100)).await;
    }
    None
}

/// The first link in a message's text part; the one carrying a token when there is one.
#[must_use]
pub fn link_in(message: &Received) -> Option<String> {
    let parsed = mail_client::message::parse(&message.data);
    let links: Vec<String> = parsed
        .text
        .split_whitespace()
        .filter(|w| w.starts_with("http://") || w.starts_with("https://"))
        .map(|w| w.trim_end_matches(['.', ',', ')', '>']).to_string())
        .collect();
    links
        .iter()
        .find(|l| l.contains("token="))
        .or_else(|| links.first())
        .cloned()
}

/// The log line gatehouse's `LoggingSender` would have written for this message, which is what the gatehouse steps read. The
/// message really went through Mailroom and SMTP; this only puts it where the steps look.
fn log_line(message: &Received) -> Option<String> {
    let kind = message.header("x-forge-mail")?;
    let to = message.to.first()?;
    let link = link_in(message).unwrap_or_default();
    Some(match kind.as_str() {
        "verification" => {
            format!("email({kind}) to={to} user=: visit {link} to verify this address")
        }
        "password-reset" => {
            format!("email({kind}) to={to} user=: visit {link} to choose a new password")
        }
        "invite" => format!("email({kind}) to={to} user=: visit {link} to accept the invitation"),
        "email-change" => {
            format!("email({kind}) to={to} user=: visit {link} to confirm the new address")
        }
        "notification" => {
            let unsubscribe = message
                .header("list-unsubscribe")
                .map(|h| h.trim_matches(['<', '>']).to_string())
                .unwrap_or_default();
            format!("email({kind}) to={to} user=: notification (unsubscribe: {unsubscribe})")
        }
        _ => format!("email({kind}) to={to} user="),
    })
}

/// Copies new messages of the sending server into gatehouse's log, in the shape its own lines have.
async fn tap(infra: Arc<Infra>) {
    let inbox = INBOX.get_or_init(Inbox::default);
    loop {
        let all = infra.smtp.received();
        let from = {
            let mut seen = inbox.seen.lock().unwrap();
            let from = *seen;
            *seen = all.len();
            from
        };
        for message in all.iter().skip(from) {
            if let Some(line) = log_line(message) {
                crate::services::gatehouse_log().lock().unwrap().push(line);
            }
        }
        sleep(Duration::from_millis(50)).await;
    }
}
