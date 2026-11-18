//! The administrator's panel: stats, settings, mailboxes (never what is in them), the services' deliveries, the audit log.

use crate::app::App;
use crate::error::{Error, Result, now};
use crate::settings::{self, Stored};
use mailroom_core::admin::{
    AdminMailbox, AuditEntry, Delivery, Page, Settings, SettingsInput, Stats,
};

/// Most rows in one page.
pub const MAX_PAGE: i64 = 200;

fn clamp(offset: i64, limit: i64) -> (i64, i64) {
    (offset.max(0), limit.clamp(1, MAX_PAGE))
}

/// The panel's front page.
///
/// # Errors
///
/// A database failure.
pub async fn stats(app: &App) -> Result<Stats> {
    let (messages, bytes) = app.store.message_totals().await?;
    let (delivered, failed) = app.store.delivery_counts(now() - 86_400).await?;
    let budget = i64::from(app.settings().daily_budget);
    Ok(Stats {
        mailboxes: u32::try_from(app.store.mailbox_count().await?).unwrap_or(u32::MAX),
        messages: u32::try_from(messages).unwrap_or(u32::MAX),
        bytes: u64::try_from(bytes).unwrap_or(0),
        delivered_24h: u32::try_from(delivered).unwrap_or(u32::MAX),
        failed_24h: u32::try_from(failed).unwrap_or(u32::MAX),
        budget_left: u32::try_from((budget - delivered).max(0)).unwrap_or(0),
    })
}

/// The settings as the panel shows them.
///
/// # Errors
///
/// Never, today; the type leaves room for settings that must be read from somewhere that can fail.
pub fn settings(app: &App) -> Result<Settings> {
    Ok(app.settings().public(&app.config.domain))
}

fn within(name: &str, value: u32, low: u32, high: u32) -> Result<u32> {
    if (low..=high).contains(&value) {
        Ok(value)
    } else {
        Err(Error::Invalid(format!(
            "{name} must be from {low} to {high}"
        )))
    }
}

/// Changes the settings (the parts that are given).
///
/// # Errors
///
/// [`Error::Invalid`] for a value out of range; nothing is changed then.
pub async fn update_settings(app: &App, actor: &str, input: &SettingsInput) -> Result<Settings> {
    let _guard = app.settings_lock.lock().await;
    let mut next: Stored = app.settings();
    let mut changed: Vec<&str> = Vec::new();
    if let Some(v) = input.daily_budget {
        next.daily_budget = within("the daily budget", v, 1, 100_000)?;
        changed.push("daily budget");
    }
    if let Some(v) = input.hourly_per_source {
        next.hourly_per_source = within("the hourly limit per service", v, 1, 100_000)?;
        changed.push("hourly per service");
    }
    if let Some(v) = input.hourly_per_person {
        next.hourly_per_person = within("the hourly limit per person", v, 1, 10_000)?;
        changed.push("hourly per person");
    }
    if let Some(v) = input.quota_mb {
        next.quota_mb = within("the mailbox size", v, 1, 100_000)?;
        changed.push("mailbox size");
    }
    settings::save(&app.store, &next).await?;
    app.set_settings(next.clone());
    app.store
        .audit(actor, "settings", "settings", &changed.join(", "))
        .await?;
    Ok(next.public(&app.config.domain))
}

/// The mailboxes, with how full they are.
///
/// # Errors
///
/// A database failure.
pub async fn mailboxes(app: &App, offset: i64, limit: i64) -> Result<Page<AdminMailbox>> {
    let (offset, limit) = clamp(offset, limit);
    let (rows, total) = app.store.mailboxes_usage(offset, limit).await?;
    Ok(Page {
        items: rows
            .into_iter()
            .map(|u| AdminMailbox {
                owner: u.mailbox.owner,
                address: u.mailbox.address,
                messages: u32::try_from(u.messages).unwrap_or(u32::MAX),
                bytes: u64::try_from(u.bytes).unwrap_or(0),
                created: u.mailbox.created,
                last_seen: u.mailbox.last_seen,
                disabled: u.mailbox.disabled,
            })
            .collect(),
        total: u32::try_from(total).unwrap_or(u32::MAX),
    })
}

/// Switches a mailbox off or on.
///
/// # Errors
///
/// [`Error::NotFound`] for an unknown mailbox.
pub async fn set_disabled(app: &App, actor: &str, owner: &str, disabled: bool) -> Result<()> {
    if !app.store.mailbox_set_disabled(owner, disabled).await? {
        return Err(Error::NotFound);
    }
    app.store
        .audit(
            actor,
            if disabled { "disable" } else { "enable" },
            owner,
            "",
        )
        .await
}

/// Deletes a mailbox with everything in it.
///
/// # Errors
///
/// [`Error::NotFound`] for an unknown mailbox.
pub async fn delete_mailbox(app: &App, actor: &str, owner: &str) -> Result<()> {
    if !app.store.mailbox_delete(owner).await? {
        return Err(Error::NotFound);
    }
    app.store.audit(actor, "delete-mailbox", owner, "").await
}

/// What the services delivered, newest first.
///
/// # Errors
///
/// A database failure.
pub async fn deliveries(app: &App, offset: i64, limit: i64) -> Result<Page<Delivery>> {
    let (offset, limit) = clamp(offset, limit);
    let (items, total) = app.store.deliveries_list(offset, limit).await?;
    Ok(Page {
        items,
        total: u32::try_from(total).unwrap_or(u32::MAX),
    })
}

/// What administrators did, newest first.
///
/// # Errors
///
/// A database failure.
pub async fn audit(app: &App, offset: i64, limit: i64) -> Result<Page<AuditEntry>> {
    let (offset, limit) = clamp(offset, limit);
    let (items, total) = app.store.audit_list(offset, limit).await?;
    Ok(Page {
        items,
        total: u32::try_from(total).unwrap_or(u32::MAX),
    })
}
