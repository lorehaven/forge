//! Turning a stored mailbox into connection settings.

use crate::app::App;
use crate::error::{Error, Result};
use crate::store::accounts::AccountRow;
use crate::vault::account_context;
use mail_client::imap::{self, Auth};
use mail_client::presets::Security;
use mail_client::smtp;

fn security(name: &str) -> Security {
    Security::parse(name).unwrap_or(Security::Tls)
}

fn roots(pem: &str) -> Vec<String> {
    if pem.trim().is_empty() {
        Vec::new()
    } else {
        vec![pem.to_string()]
    }
}

/// The password of a mailbox, opened. A password sealed under the previous key is sealed again under the current one.
///
/// # Errors
///
/// When the stored secret cannot be opened (the vault key changed without a rotation).
pub async fn password(app: &App, row: &AccountRow) -> Result<String> {
    let context = account_context(&row.id, &row.owner);
    let (password, old_key) = app.vault.open(&context, &row.secret)?;
    if old_key {
        let sealed = app.vault.seal(&context, &password);
        app.store.account_set_secret(&row.id, &sealed).await?;
    }
    Ok(password)
}

/// IMAP settings for a mailbox.
#[must_use]
pub fn imap_config(app: &App, row: &AccountRow, password: &str) -> imap::Config {
    let mut config = imap::Config::new(
        &row.imap_host,
        u16::try_from(row.imap_port).unwrap_or(993),
        security(&row.imap_security),
        &row.username,
        Auth::Password(password.to_string()),
    );
    config.policy = app.policy();
    config.tls_roots_pem = roots(&row.tls_roots);
    config.timeout = std::time::Duration::from_secs(30);
    config
}

/// SMTP settings for a mailbox.
#[must_use]
pub fn smtp_config(app: &App, row: &AccountRow, password: &str) -> smtp::Config {
    let mut config = smtp::Config::new(
        &row.smtp_host,
        u16::try_from(row.smtp_port).unwrap_or(465),
        security(&row.smtp_security),
        &row.username,
        Auth::Password(password.to_string()),
    );
    config.policy = app.policy();
    config.tls_roots_pem = roots(&row.tls_roots);
    config
}

/// Opens an IMAP session for a mailbox.
///
/// # Errors
///
/// [`Error::Upstream`] with a sentence about what failed (never the password).
pub async fn open_imap(app: &App, row: &AccountRow) -> Result<imap::Client> {
    let password = password(app, row).await?;
    imap::Client::connect(&imap_config(app, row, &password))
        .await
        .map_err(|e| Error::Upstream(upstream_text(&e)))
}

/// What a person is told about an IMAP failure.
#[must_use]
pub fn upstream_text(error: &imap::Error) -> String {
    match error {
        imap::Error::Auth(_) => "the mail server refused the login: check the password (Gmail and others need an app password)".into(),
        imap::Error::Tls(_) => "the connection to the mail server is not secure: its certificate was not accepted".into(),
        imap::Error::Refused(refusal) => format!("this server cannot be used: {refusal}"),
        imap::Error::Timeout => "the mail server did not answer in time".into(),
        imap::Error::Insecure => "the mail server offers no secure connection".into(),
        imap::Error::Io(_) => "could not connect to the mail server".into(),
        imap::Error::Bye(_) => "the mail server closed the connection".into(),
        imap::Error::Protocol(_) | imap::Error::Bad(_) => "the mail server answered something unexpected".into(),
        imap::Error::No(text) => format!("the mail server refused: {}", short(text)),
    }
}

/// A server's words, cut and stripped of control characters, for showing.
#[must_use]
pub fn short(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).take(160).collect()
}
