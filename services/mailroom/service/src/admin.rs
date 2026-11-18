//! The administrator's side: settings, the sending server, everyone's mailboxes (never their mail), the queue, the audit log.

use crate::app::App;
use crate::error::{Error, Result, now};
use crate::settings::{self, Stored, StoredTransport};
use crate::store::outbox::OutboxRow;
use mail_client::imap::Auth;
use mail_client::presets::Security;
use mail_client::{netguard, smtp};
use mailroom_core::admin::{
    AdminAccount, Identity, OutboxItem, Page, Settings, SettingsInput, Stats, TransportInput,
};
use mailroom_core::{Step, TestResult};

/// The longest list an administrator can ask for at once.
pub const MAX_PAGE: i64 = 200;

/// The settings the panel shows.
///
/// # Errors
///
/// A database failure.
pub async fn settings(app: &App) -> Result<Settings> {
    let stored = app.settings();
    let has_password = settings::transport_secret(&app.store).await?.is_some();
    Ok(stored.public(has_password))
}

fn bad(text: &str) -> Error {
    Error::Invalid(text.to_string())
}

fn check_private_host(rule: &str) -> bool {
    let host = rule.strip_prefix("*.").unwrap_or(rule);
    netguard::valid_host(host) && !rule.contains('[')
}

/// Checks and applies a change to the settings.
///
/// # Errors
///
/// [`Error::Invalid`] naming what is wrong; nothing is changed then.
pub async fn update_settings(app: &App, actor: &str, input: &SettingsInput) -> Result<Settings> {
    let _one_at_a_time = app.settings_lock.lock().await;
    let mut next: Stored = app.settings();
    let mut changed: Vec<&str> = Vec::new();
    let mut new_password: Option<String> = None;

    if let Some(t) = &input.transport {
        let transport = transport_from(t, next.transport.as_ref())?;
        // Pointing the stored login at another server or user without typing the password again would hand it to that server.
        if let Some(old) = &next.transport
            && (old.host != transport.host
                || old.port != transport.port
                || old.username != transport.username)
            && t.password.is_empty()
        {
            return Err(bad(
                "changing the server, port or user name needs the password typed again",
            ));
        }
        if next
            .transport
            .as_ref()
            .is_some_and(|old| old.host != transport.host)
        {
            changed.push("transport host changed");
        }
        if !t.password.is_empty() {
            if t.password.len() > 512 || t.password.contains(['\r', '\n', '\0']) {
                return Err(bad("the password is not valid"));
            }
            new_password = Some(t.password.clone());
        }
        next.transport = Some(transport);
        changed.push("transport");
    }
    if let Some(identities) = &input.identities {
        if identities.len() > 20 {
            return Err(bad("at most 20 sender addresses"));
        }
        let mut cleaned: Vec<Identity> = Vec::new();
        for identity in identities {
            let address = identity.address.trim();
            if !mailroom_core::draft::plausible_address(address) {
                return Err(bad("a sender address is not valid"));
            }
            if identity.name.len() > 100 || identity.name.chars().any(char::is_control) {
                return Err(bad("a sender name is not valid"));
            }
            if cleaned
                .iter()
                .any(|i| i.address.eq_ignore_ascii_case(address))
            {
                return Err(bad("a sender address is listed twice"));
            }
            cleaned.push(Identity {
                address: address.to_string(),
                name: identity.name.trim().to_string(),
                default: identity.default,
            });
        }
        // Exactly one default when there are any.
        if !cleaned.is_empty() {
            let first_default = cleaned.iter().position(|i| i.default).unwrap_or(0);
            for (i, identity) in cleaned.iter_mut().enumerate() {
                identity.default = i == first_default;
            }
        }
        next.identities = cleaned;
        changed.push("identities");
    }
    if let Some(hosts) = &input.private_hosts {
        if hosts.len() > 50 || hosts.iter().any(|h| !check_private_host(h.trim())) {
            return Err(bad("a private host is not valid (a name, or *.suffix)"));
        }
        next.private_hosts = hosts
            .iter()
            .map(|h| h.trim().to_ascii_lowercase())
            .collect();
        changed.push("private_hosts");
    }
    if let Some(ports) = &input.extra_ports {
        if ports.len() > 20 || ports.contains(&0) {
            return Err(bad("a port is not valid"));
        }
        next.extra_ports.clone_from(ports);
        changed.push("extra_ports");
    }
    if let Some(n) = input.daily_budget {
        if !(1..=100_000).contains(&n) {
            return Err(bad("the daily budget must be 1 to 100000"));
        }
        next.daily_budget = n;
        changed.push("daily_budget");
    }
    if let Some(n) = input.hourly_per_source {
        if !(1..=100_000).contains(&n) {
            return Err(bad("the hourly limit must be 1 to 100000"));
        }
        next.hourly_per_source = n;
        changed.push("hourly_per_source");
    }
    if let Some(n) = input.max_accounts_per_user {
        if !(1..=100).contains(&n) {
            return Err(bad("mailboxes per person must be 1 to 100"));
        }
        next.max_accounts_per_user = n;
        changed.push("max_accounts_per_user");
    }
    if let Some(n) = input.sync_interval_secs {
        if !(30..=3600).contains(&n) {
            return Err(bad("the refresh interval must be 30 to 3600 seconds"));
        }
        next.sync_interval_secs = n;
        changed.push("sync_interval_secs");
    }

    if let Some(password) = new_password {
        settings::save_transport_secret(&app.store, &app.vault, &password).await?;
    }
    settings::save(&app.store, &next).await?;
    app.set_settings(next);
    // The names of what changed, never values: the audit log is read by administrators and may be exported.
    app.store
        .audit(actor, "settings.update", "", &changed.join(", "))
        .await?;
    self::settings(app).await
}

fn transport_from(
    input: &TransportInput,
    existing: Option<&StoredTransport>,
) -> Result<StoredTransport> {
    if !netguard::valid_host(input.host.trim()) {
        return Err(bad("the sending server name is not valid"));
    }
    if input.port == 0 {
        return Err(bad("the sending server port is not valid"));
    }
    if Security::parse(&input.security).is_none() {
        return Err(bad("choose TLS or STARTTLS for the sending server"));
    }
    if input.username.len() > 254 || input.username.chars().any(char::is_control) {
        return Err(bad("the sending server user name is not valid"));
    }
    let roots = if input.tls_roots_pem.trim().is_empty() {
        // Blank keeps what is stored, so saving the form does not drop a pinned certificate.
        existing
            .map(|e| e.tls_roots_pem.clone())
            .unwrap_or_default()
    } else if input.tls_roots_pem.len() > 20_000
        || !input.tls_roots_pem.contains("BEGIN CERTIFICATE")
    {
        return Err(bad("the certificate is not a valid PEM"));
    } else {
        input.tls_roots_pem.trim().to_string()
    };
    Ok(StoredTransport {
        host: input.host.trim().to_ascii_lowercase(),
        port: input.port,
        security: input.security.clone(),
        username: input.username.trim().to_string(),
        tls_roots_pem: roots,
    })
}

/// Logs in to the sending server without sending anything.
///
/// # Errors
///
/// [`Error::Invalid`] when no server is configured.
pub async fn test_transport(app: &App, actor: &str) -> Result<TestResult> {
    app.limits
        .hit("test-transport", 12, 60, now())
        .map_err(Error::Limited)?;
    let settings = app.settings();
    let transport = settings
        .transport
        .ok_or_else(|| bad("no sending server is configured"))?;
    let password = settings::open_transport_secret(&app.store, &app.vault).await?;
    let mut config = smtp::Config::new(
        &transport.host,
        transport.port,
        Security::parse(&transport.security).unwrap_or(Security::StartTls),
        &transport.username,
        Auth::Password(password),
    );
    config.policy = app.policy();
    if !transport.tls_roots_pem.is_empty() {
        config.tls_roots_pem = vec![transport.tls_roots_pem];
    }
    let step = match smtp::test(&config).await {
        Ok(()) => Step {
            ok: true,
            error: String::new(),
        },
        Err(e) => Step {
            ok: false,
            error: crate::connect::short(&e.message),
        },
    };
    app.store
        .audit(
            actor,
            "transport.test",
            &transport.host,
            if step.ok { "ok" } else { "failed" },
        )
        .await?;
    Ok(TestResult {
        imap: Step {
            ok: true,
            error: String::new(),
        },
        smtp: step,
    })
}

/// Everyone's mailboxes, as metadata.
///
/// # Errors
///
/// A database failure.
pub async fn accounts(app: &App, offset: i64, limit: i64) -> Result<Page<AdminAccount>> {
    let rows = app
        .store
        .accounts_all(offset.max(0), limit.clamp(1, MAX_PAGE))
        .await?;
    let (total, _) = app.store.accounts_counts().await?;
    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        items.push(AdminAccount {
            messages: u32::try_from(app.store.account_message_count(&row.id).await?).unwrap_or(0),
            id: row.id,
            owner: row.owner,
            label: row.label,
            address: row.address,
            imap_host: row.imap_host,
            state: if row.disabled {
                "disabled".into()
            } else {
                row.state
            },
            last_error: row.last_error,
            last_sync: row.last_sync,
            disabled: row.disabled,
        });
    }
    Ok(Page {
        items,
        total: u32::try_from(total).unwrap_or(0),
    })
}

/// Switches a mailbox off or on.
///
/// # Errors
///
/// [`Error::NotFound`].
pub async fn set_account_disabled(app: &App, actor: &str, id: &str, disabled: bool) -> Result<()> {
    if !app.store.account_set_disabled(id, disabled).await? {
        return Err(Error::NotFound);
    }
    app.store
        .audit(
            actor,
            if disabled {
                "account.disable"
            } else {
                "account.enable"
            },
            id,
            "",
        )
        .await?;
    if !disabled {
        app.hub.request(id);
    }
    Ok(())
}

/// Removes a mailbox and its cache.
///
/// # Errors
///
/// [`Error::NotFound`].
pub async fn delete_account(app: &App, actor: &str, id: &str) -> Result<()> {
    if !app.store.account_delete(id).await? {
        return Err(Error::NotFound);
    }
    app.hub.forget(id);
    app.store.audit(actor, "account.delete", id, "").await
}

fn outbox_item(row: OutboxRow) -> OutboxItem {
    // A person's own mail is theirs: the panel shows that it went, not to whom or about what.
    let private = row.kind == "user";
    OutboxItem {
        id: row.id,
        source: row.source,
        kind: row.kind,
        recipients: if private {
            Vec::new()
        } else {
            quench_serde::json::from_str(&row.recipients).unwrap_or_default()
        },
        subject: if private { String::new() } else { row.subject },
        status: row.status,
        attempts: u32::try_from(row.attempts).unwrap_or(0),
        last_error: if private {
            row.last_error
                .map(|e| crate::connect::short(&e).chars().take(60).collect())
        } else {
            row.last_error
        },
        created: row.created,
        next_attempt: row.next_attempt,
        sent: row.sent,
    }
}

/// The queue and its history. No message bodies.
///
/// # Errors
///
/// [`Error::Invalid`] for an unknown status.
pub async fn outbox(
    app: &App,
    status: Option<&str>,
    offset: i64,
    limit: i64,
) -> Result<Page<OutboxItem>> {
    if let Some(s) = status
        && !s.is_empty()
        && !matches!(s, "queued" | "sending" | "sent" | "failed")
    {
        return Err(bad("unknown status"));
    }
    let (rows, total) = app
        .store
        .outbox_list(status, offset.max(0), limit.clamp(1, MAX_PAGE))
        .await?;
    Ok(Page {
        items: rows.into_iter().map(outbox_item).collect(),
        total: u32::try_from(total).unwrap_or(0),
    })
}

/// Tries a queued message again right away.
///
/// # Errors
///
/// [`Error::NotFound`] when it is not waiting.
pub async fn retry(app: &App, actor: &str, id: &str) -> Result<()> {
    if !app.store.outbox_retry_now(id, now()).await? {
        return Err(Error::NotFound);
    }
    app.store.audit(actor, "outbox.retry", id, "").await
}

/// Removes a message from the queue or the history.
///
/// # Errors
///
/// [`Error::NotFound`].
pub async fn remove_outbox(app: &App, actor: &str, id: &str) -> Result<()> {
    if !app.store.outbox_delete(id).await? {
        return Err(Error::NotFound);
    }
    app.store.audit(actor, "outbox.delete", id, "").await
}

/// The numbers on the front page.
///
/// # Errors
///
/// A database failure.
pub async fn stats(app: &App) -> Result<Stats> {
    let (accounts, failing) = app.store.accounts_counts().await?;
    let (queued, sent, failed) = app.store.outbox_stats(now() - 86_400).await?;
    let used = app
        .store
        .outbox_count_since("", "service", now() - 86_400)
        .await?;
    Ok(Stats {
        accounts: u32::try_from(accounts).unwrap_or(0),
        accounts_failing: u32::try_from(failing).unwrap_or(0),
        messages: u32::try_from(app.store.messages_count().await?).unwrap_or(0),
        queued: u32::try_from(queued).unwrap_or(0),
        sent_24h: u32::try_from(sent).unwrap_or(0),
        failed_24h: u32::try_from(failed).unwrap_or(0),
        budget_left: u32::try_from((i64::from(app.settings().daily_budget) - used).max(0))
            .unwrap_or(0),
    })
}
