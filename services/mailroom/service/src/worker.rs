//! The background work: sending what is queued, and looking at mailboxes.

use crate::app::App;
use crate::connect;
use crate::error::{Error, Result, now};
use crate::settings as stored;
use crate::store::outbox::OutboxRow;
use mail_client::imap::Auth;
use mail_client::presets::Security;
use mail_client::smtp;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;

/// Tries before a message is given up on.
pub const MAX_ATTEMPTS: i32 = 6;
/// How long a finished message stays in the history, and a stuck one before it is queued again.
const HISTORY_DAYS: i64 = 30;
const STUCK_SECS: i64 = 600;

/// Seconds to wait after the `attempts`-th failure.
#[must_use]
pub const fn backoff(attempts: i32) -> i64 {
    match attempts {
        ..=1 => 60,
        2 => 5 * 60,
        3 => 30 * 60,
        4 => 2 * 3600,
        _ => 6 * 3600,
    }
}

/// Starts the loops. They stop when the process does.
pub fn spawn(app: &App) {
    let sender = app.clone();
    tokio::spawn(async move {
        let mut housekeeping = 0;
        loop {
            // Until nothing is due: a burst of mail goes out in one go, not one batch per poll.
            loop {
                match send_due(&sender).await {
                    Ok(0) => break,
                    Ok(_) => {}
                    Err(e) => {
                        quench_log::warn!("outbox run failed: {e}");
                        break;
                    }
                }
            }
            housekeeping += 1;
            if housekeeping >= 1800 {
                housekeeping = 0;
                if let Err(e) = prune(&sender).await {
                    quench_log::warn!("housekeeping failed: {e}");
                }
            }
            // Woken at once by a new message; a poll every few seconds picks up retries that have come due.
            let _ = tokio::time::timeout(Duration::from_secs(2), sender.wake.notified()).await;
        }
    });
    let syncer = app.clone();
    tokio::spawn(async move {
        let slots = Arc::new(Semaphore::new(4));
        loop {
            if let Err(e) = sync_due(&syncer, &slots).await {
                quench_log::warn!("sync scheduler failed: {e}");
            }
            tokio::time::sleep(Duration::from_secs(15)).await;
        }
    });
}

async fn prune(app: &App) -> Result<()> {
    let now = now();
    app.store.outbox_prune(now - HISTORY_DAYS * 86_400).await?;
    app.store.uploads_prune(now - 86_400).await?;
    app.store.bodies_prune(now - 60 * 86_400).await?;
    app.store.outbox_requeue_stuck(now - STUCK_SECS).await?;
    Ok(())
}

/// Starts a sync for each mailbox that is due, a few at a time.
///
/// # Errors
///
/// A database failure.
pub async fn sync_due(app: &App, slots: &Arc<Semaphore>) -> Result<()> {
    let interval = i64::from(app.settings().sync_interval_secs.max(30));
    let requested = app.hub.take_requested();
    let now = now();
    let mut offset = 0;
    loop {
        let page = app.store.accounts_all(offset, 200).await?;
        if page.is_empty() {
            break;
        }
        offset += 200;
        for row in page {
            if row.disabled {
                continue;
            }
            // A mailbox that fails is looked at less and less often (up to every quarter hour).
            let wait = if row.state == "error" {
                (interval * 4).min(900)
            } else {
                interval
            };
            let due = row.last_sync.is_none_or(|t| now - t >= wait) || requested.contains(&row.id);
            if !due {
                continue;
            }
            let Ok(permit) = slots.clone().try_acquire_owned() else {
                // All slots busy: ask again next time.
                app.hub.request(&row.id);
                continue;
            };
            let app = app.clone();
            tokio::spawn(async move {
                if let Err(e) = crate::sync::sync_account(&app, &row.id).await {
                    quench_log::warn!("sync of {} failed: {e}", row.id);
                }
                drop(permit);
            });
        }
    }
    Ok(())
}

/// Sends the messages that are due. Returns how many it looked at.
///
/// # Errors
///
/// A database failure.
pub async fn send_due(app: &App) -> Result<usize> {
    let due = app.store.outbox_claim(now(), 20).await?;
    let count = due.len();
    // Each message has its own connection; do not make one wait for another's slow server.
    let mut tasks = tokio::task::JoinSet::new();
    for row in due {
        let app = app.clone();
        tasks.spawn(async move { deliver(&app, row).await });
    }
    while let Some(done) = tasks.join_next().await {
        match done {
            Ok(Ok(())) => {}
            Ok(Err(e)) => return Err(e),
            Err(e) => return Err(Error::Internal(format!("a send task failed: {e}"))),
        }
    }
    Ok(count)
}

async fn deliver(app: &App, row: OutboxRow) -> Result<()> {
    let Some(raw) = row.raw.clone() else {
        return app
            .store
            .outbox_mark_failed(&row.id, row.attempts, "the message body is gone")
            .await;
    };
    let recipients: Vec<String> = quench_serde::json::from_str(&row.recipients).unwrap_or_default();
    let built = match built(&row, &recipients, raw) {
        Ok(built) => built,
        Err(text) => {
            return app
                .store
                .outbox_mark_failed(&row.id, row.attempts, &text)
                .await;
        }
    };
    let config = match config_for(app, &row).await {
        Ok(config) => config,
        Err(Error::Db(e)) => return Err(Error::Db(e)),
        Err(e) => {
            let (_, text) = e.parts();
            return fail(app, &row, true, &text).await;
        }
    };
    match smtp::send(&config, &built).await {
        Ok(()) => {
            app.store.outbox_mark_sent(&row.id, now()).await?;
            if row.kind == "user" {
                after_user_send(app, &row, &built.raw).await;
            }
            Ok(())
        }
        Err(e) => fail(app, &row, e.transient, &connect::short(&e.message)).await,
    }
}

fn built(
    row: &OutboxRow,
    recipients: &[String],
    raw: Vec<u8>,
) -> std::result::Result<mail_client::compose::Built, String> {
    let from = row
        .envelope_from
        .parse()
        .map_err(|_| "the sender address is not valid".to_string())?;
    let to: Vec<_> = recipients.iter().filter_map(|r| r.parse().ok()).collect();
    if to.is_empty() {
        return Err("there are no valid recipients".into());
    }
    Ok(mail_client::compose::Built {
        raw,
        envelope: quench_mail::Envelope { from, to },
        message_id: String::new(),
    })
}

async fn fail(app: &App, row: &OutboxRow, transient: bool, text: &str) -> Result<()> {
    let attempts = row.attempts + 1;
    quench_log::warn!(
        "sending {} failed (attempt {attempts}, transient {transient}): {text}",
        row.id
    );
    if transient && attempts < MAX_ATTEMPTS {
        app.store
            .outbox_mark_retry(&row.id, attempts, now() + backoff(attempts), text)
            .await
    } else {
        app.store.outbox_mark_failed(&row.id, attempts, text).await
    }
}

async fn config_for(app: &App, row: &OutboxRow) -> Result<smtp::Config> {
    if row.kind == "user" {
        let id = row.account_id.as_deref().ok_or(Error::NotFound)?;
        let account = app.store.account_get(id).await?.ok_or(Error::NotFound)?;
        if account.disabled {
            return Err(Error::Upstream("this mailbox is disabled".into()));
        }
        let password = connect::password(app, &account).await?;
        return Ok(connect::smtp_config(app, &account, &password));
    }
    let settings = app.settings();
    let transport = settings
        .transport
        .ok_or_else(|| Error::Upstream("no sending server is configured".into()))?;
    let password = stored::open_transport_secret(&app.store, &app.vault).await?;
    let mut config = smtp::Config::new(
        &transport.host,
        transport.port,
        Security::parse(&transport.security).unwrap_or(Security::StartTls),
        &transport.username,
        Auth::Password(password),
    );
    config.policy = app.policy();
    if !transport.tls_roots_pem.trim().is_empty() {
        config.tls_roots_pem = vec![transport.tls_roots_pem];
    }
    Ok(config)
}

/// A copy in Sent, and the answered flag on the message that was replied to. Neither can fail the send: it has gone.
async fn after_user_send(app: &App, row: &OutboxRow, raw: &[u8]) {
    let Some(account_id) = row.account_id.as_deref() else {
        return;
    };
    let Ok(Some(account)) = app.store.account_get(account_id).await else {
        return;
    };
    if !account.save_sent && row.reply_to_message.is_none() {
        return;
    }
    let result: Result<()> = async {
        let mut client = connect::open_imap(app, &account).await?;
        let upstream = |e: mail_client::imap::Error| Error::Upstream(connect::upstream_text(&e));
        if account.save_sent {
            // Where Sent is comes from the server, not from the cache: the mailbox may have been added a moment ago.
            let folders = client.list_folders().await.map_err(upstream)?;
            if let Some(sent) = folders
                .iter()
                .find(|f| f.selectable() && f.role() == "sent")
            {
                client
                    .append(&sent.path, &["\\Seen"], raw.to_vec())
                    .await
                    .map_err(upstream)?;
                app.hub.request(&account.id);
            }
        }
        if let Some(message_id) = &row.reply_to_message
            && let Some(message) = app.store.message_get(message_id).await?
            && let Some(folder) = app.store.folder_get(&message.folder_id).await?
            && let Ok(uid) = u32::try_from(message.uid)
        {
            client.select(&folder.path, false).await.map_err(upstream)?;
            client
                .store_flags(&uid.to_string(), "+", &["\\Answered"])
                .await
                .map_err(upstream)?;
            app.store
                .message_set_state(&message.id, None, None, Some(true))
                .await?;
        }
        client.logout().await;
        Ok(())
    }
    .await;
    if let Err(e) = result {
        quench_log::warn!("after sending {}: {e}", row.id);
    }
}
