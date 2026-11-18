//! A person's mailboxes: adding, changing, testing, removing.

use crate::app::App;
use crate::connect;
use crate::error::{Error, Result, new_id, now};
use crate::store::accounts::AccountRow;
use crate::vault::account_context;
use mail_client::{netguard, presets, smtp};
use mailroom_core::{Account, AccountInput, Preset, Step, TestResult};

/// The longest password accepted (app passwords are 16 characters; tokens can be long).
const MAX_PASSWORD: usize = 512;

/// The API's view of a row. Never the password.
#[must_use]
pub fn model(row: &AccountRow, unread: i64) -> Account {
    Account {
        id: row.id.clone(),
        label: row.label.clone(),
        address: row.address.clone(),
        username: row.username.clone(),
        imap_host: row.imap_host.clone(),
        imap_port: u16::try_from(row.imap_port).unwrap_or(0),
        imap_security: row.imap_security.clone(),
        smtp_host: row.smtp_host.clone(),
        smtp_port: u16::try_from(row.smtp_port).unwrap_or(0),
        smtp_security: row.smtp_security.clone(),
        state: if row.disabled {
            "disabled".into()
        } else {
            row.state.clone()
        },
        last_error: row.last_error.clone(),
        last_sync: row.last_sync,
        unread: u32::try_from(unread).unwrap_or(0),
        signature: row.signature.clone(),
        custom_certificate: !row.tls_roots.is_empty(),
        save_sent: row.save_sent,
    }
}

/// The provider suggestions for the add form.
#[must_use]
pub fn presets() -> Vec<Preset> {
    presets::PRESETS
        .iter()
        .chain(std::iter::once(&presets::PROTON_BRIDGE))
        .map(|p| Preset {
            id: p.id.to_string(),
            name: p.name.to_string(),
            imap_host: p.imap_host.to_string(),
            imap_port: p.imap_port,
            imap_security: p.imap_security.name().to_string(),
            smtp_host: p.smtp_host.to_string(),
            smtp_port: p.smtp_port,
            smtp_security: p.smtp_security.name().to_string(),
            note: p.note.to_string(),
            // Gmail and Outlook put what you send in Sent themselves; a second copy would be a duplicate.
            save_sent: !matches!(p.id, "gmail" | "outlook"),
        })
        .collect()
}

/// Checks what a person typed. Nothing here touches the network.
///
/// # Errors
///
/// [`Error::Invalid`] naming the first problem.
pub fn validate(input: &AccountInput, needs_password: bool) -> Result<()> {
    let bad = |text: &str| Err(Error::Invalid(text.to_string()));
    if !mailroom_core::draft::plausible_address(&input.address) {
        return bad("that is not an email address");
    }
    if input.label.trim().is_empty()
        || input.label.chars().count() > 60
        || input.label.chars().any(char::is_control)
    {
        return bad("the name must be 1 to 60 characters");
    }
    if input.username.trim().is_empty()
        || input.username.len() > 254
        || input.username.chars().any(char::is_control)
    {
        return bad("the user name is missing or invalid");
    }
    if needs_password && input.password.is_empty() {
        return bad("a password is required");
    }
    if input.password.len() > MAX_PASSWORD
        || input
            .password
            .chars()
            .any(|c| c == '\0' || c == '\r' || c == '\n')
    {
        return bad("the password is not valid");
    }
    for host in [&input.imap_host, &input.smtp_host] {
        if !netguard::valid_host(host.trim()) {
            return bad("a server name is not valid");
        }
    }
    for port in [input.imap_port, input.smtp_port] {
        if port == 0 {
            return bad("a port is not valid");
        }
    }
    for security in [&input.imap_security, &input.smtp_security] {
        if presets::Security::parse(security).is_none() {
            return bad("choose TLS or STARTTLS for each server");
        }
    }
    if input.signature.len() > 4000 {
        return bad("the signature is too long");
    }
    if !input.tls_roots_pem.trim().is_empty() {
        if input.tls_roots_pem.len() > 20_000 {
            return bad("the certificate is too large");
        }
        match certificate_count(&input.tls_roots_pem) {
            Ok(0) | Err(()) => return bad("the certificate is not a valid PEM"),
            Ok(_) => {}
        }
    }
    Ok(())
}

/// How many certificates the PEM holds (a sanity bound, not a parse: the client parses it when it connects).
fn certificate_count(pem: &str) -> std::result::Result<usize, ()> {
    let begin = pem.matches("-----BEGIN CERTIFICATE-----").count();
    let end = pem.matches("-----END CERTIFICATE-----").count();
    if begin == 0 || begin != end || begin > 5 {
        return Err(());
    }
    Ok(begin)
}

fn row_from_input(
    app: &App,
    owner: &str,
    id: &str,
    input: &AccountInput,
    secret: String,
    created: i64,
) -> AccountRow {
    let _ = app;
    AccountRow {
        id: id.to_string(),
        owner: owner.to_string(),
        label: input.label.trim().to_string(),
        address: input.address.trim().to_string(),
        username: input.username.trim().to_string(),
        imap_host: input.imap_host.trim().to_ascii_lowercase(),
        imap_port: i32::from(input.imap_port),
        imap_security: input.imap_security.clone(),
        smtp_host: input.smtp_host.trim().to_ascii_lowercase(),
        smtp_port: i32::from(input.smtp_port),
        smtp_security: input.smtp_security.clone(),
        secret,
        tls_roots: input.tls_roots_pem.trim().to_string(),
        signature: input.signature.clone(),
        save_sent: input.save_sent,
        state: "ok".into(),
        last_error: None,
        last_sync: None,
        disabled: false,
        created,
    }
}

/// Tries a mailbox's settings: logs in to IMAP and to SMTP (without sending). Rate limited per person: this is a way to make the
/// server dial out, so it is not offered without limit.
///
/// # Errors
///
/// [`Error::Limited`] when called too often, [`Error::Invalid`] for a bad form.
pub async fn test(app: &App, owner: &str, input: &AccountInput) -> Result<TestResult> {
    app.limits
        .hit(&format!("test:{owner}"), 12, 60, now())
        .map_err(Error::Limited)?;
    validate(input, true)?;
    let row = row_from_input(app, owner, "test", input, String::new(), now());
    Ok(run_test(app, &row, &input.password).await)
}

async fn run_test(app: &App, row: &AccountRow, password: &str) -> TestResult {
    let imap =
        match mail_client::imap::Client::connect(&connect::imap_config(app, row, password)).await {
            Ok(client) => {
                client.logout().await;
                Step {
                    ok: true,
                    error: String::new(),
                }
            }
            Err(e) => Step {
                ok: false,
                error: connect::upstream_text(&e),
            },
        };
    let smtp = match smtp::test(&connect::smtp_config(app, row, password)).await {
        Ok(()) => Step {
            ok: true,
            error: String::new(),
        },
        Err(e) => Step {
            ok: false,
            error: connect::short(&e.message),
        },
    };
    TestResult { imap, smtp }
}

/// Adds a mailbox after its IMAP login works.
///
/// # Errors
///
/// [`Error::Invalid`] for a bad form or one over the quota, [`Error::Upstream`] when the mail server refuses.
pub async fn create(app: &App, owner: &str, input: &AccountInput) -> Result<Account> {
    validate(input, true)?;
    let max = i64::from(app.settings().max_accounts_per_user);
    if app.store.accounts_count_of(owner).await? >= max {
        return Err(Error::Invalid(format!(
            "at most {max} mailboxes per person"
        )));
    }
    app.limits
        .hit(&format!("test:{owner}"), 12, 60, now())
        .map_err(Error::Limited)?;
    let id = new_id();
    let secret = app
        .vault
        .seal(&account_context(&id, owner), &input.password);
    let row = row_from_input(app, owner, &id, input, secret, now());
    // The login must work before anything is kept.
    let client =
        mail_client::imap::Client::connect(&connect::imap_config(app, &row, &input.password))
            .await
            .map_err(|e| Error::Upstream(connect::upstream_text(&e)))?;
    client.logout().await;
    app.store.account_insert(&row).await?;
    app.hub.request(&row.id);
    Ok(model(&row, 0))
}

/// Changes a mailbox. A blank password keeps the stored one. The login is tried again.
///
/// # Errors
///
/// [`Error::NotFound`] when `owner` has no such mailbox.
pub async fn update(app: &App, owner: &str, id: &str, input: &AccountInput) -> Result<Account> {
    let existing = app
        .store
        .account_get_owned(id, owner)
        .await?
        .ok_or(Error::NotFound)?;
    if existing.disabled {
        return Err(Error::Forbidden);
    }
    validate(input, false)?;
    app.limits
        .hit(&format!("test:{owner}"), 12, 60, now())
        .map_err(Error::Limited)?;
    let (secret, password) = if input.password.is_empty() {
        let password = connect::password(app, &existing).await?;
        (existing.secret.clone(), password)
    } else {
        (
            app.vault.seal(&account_context(id, owner), &input.password),
            input.password.clone(),
        )
    };
    let mut row = row_from_input(app, owner, id, input, secret, existing.created);
    row.disabled = existing.disabled;
    let client = mail_client::imap::Client::connect(&connect::imap_config(app, &row, &password))
        .await
        .map_err(|e| Error::Upstream(connect::upstream_text(&e)))?;
    client.logout().await;
    app.store.account_update(&row).await?;
    app.hub.request(id);
    let unread = app.store.account_unread(id).await?;
    Ok(model(&row, unread))
}

/// A person's mailboxes.
///
/// # Errors
///
/// A database failure.
pub async fn list(app: &App, owner: &str) -> Result<Vec<Account>> {
    let rows = app.store.accounts_of(owner).await?;
    let mut out = Vec::with_capacity(rows.len());
    for row in &rows {
        out.push(model(row, app.store.account_unread(&row.id).await?));
    }
    Ok(out)
}

/// Removes a mailbox and what was cached from it.
///
/// # Errors
///
/// [`Error::NotFound`] when `owner` has no such mailbox.
pub async fn delete(app: &App, owner: &str, id: &str) -> Result<()> {
    app.store
        .account_get_owned(id, owner)
        .await?
        .ok_or(Error::NotFound)?;
    app.store.account_delete(id).await?;
    Ok(())
}
