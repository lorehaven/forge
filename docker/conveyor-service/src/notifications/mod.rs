//! Telling people how the runs they follow ended: failures, and the first success after one.
//!
//! Conveyor decides *who* and *what happened*; gatehouse decides how it reads, in which language,
//! and whether the person still wants it. Between the two sits an outbox, so a run finishing never
//! waits on mail and a gatehouse outage delays notifications instead of dropping them.

pub mod gatehouse;
pub mod outbox;
pub mod subscriptions;

use crate::domain::{Repo, Run, Status};
use crate::scheduler::projects;
use crate::scheduler::queue::{QueueError, pool, schema};
use gatehouse::{Delivery, Notifier};
use quench_auth::domain::auth::User;
use quench_auth::prelude::UserDb;
use quench_db::prelude::Db;
use sqlx::Row;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

/// Gatehouse's template ids for the two events a subscriber hears about.
pub const RUN_FAILED: &str = "conveyor.run.failed";
pub const RUN_RECOVERED: &str = "conveyor.run.recovered";

/// How long the delivery loop sleeps when nothing is due.
const IDLE_POLL: Duration = Duration::from_secs(5);

/// What the worker needs beyond the database to queue notifications.
#[derive(Clone)]
pub struct Notifications {
    pub user_db: Arc<UserDb>,
    /// With auth off there are no grants to check, so everyone may read everything.
    pub auth_enabled: bool,
}

/// The message a run's end calls for, if any. Cancelled and skipped runs say nothing; a success
/// only matters when the run before it on the same ref had failed.
pub async fn event_for(
    db: &Db,
    run: &Run,
    status: Status,
) -> Result<Option<&'static str>, QueueError> {
    match status {
        Status::Failed => Ok(Some(RUN_FAILED)),
        Status::Success => Ok(previous_status(db, run)
            .await?
            .filter(|previous| *previous == Status::Failed)
            .map(|_| RUN_RECOVERED)),
        _ => Ok(None),
    }
}

/// How the run before this one ended on the same repository and ref (cancelled ones don't count).
async fn previous_status(db: &Db, run: &Run) -> Result<Option<Status>, QueueError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!(
        "SELECT status FROM {schema}.runs \
         WHERE repo_id = $1 AND git_ref = $2 AND id <> $3 AND queued_at < $4 \
           AND status IN ('success', 'failed') \
         ORDER BY queued_at DESC LIMIT 1"
    );
    let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(&run.repo_id)
        .bind(&run.git_ref)
        .bind(&run.id)
        .bind(run.queued_at)
        .fetch_optional(pool)
        .await?;

    Ok(row
        .map(|row| row.try_get::<String, _>("status"))
        .transpose()?
        .and_then(|raw| Status::parse(&raw)))
}

/// Whether `user` may read the project a repository sits in - directly or through a project above.
pub fn may_read(user: &User, chain: &[String]) -> bool {
    !user.is_disabled()
        && (user.can("conveyor", "read")
            || chain
                .iter()
                .any(|id| user.can("conveyor", &format!("project:{id}:read"))))
}

fn short_id(id: &str) -> String {
    id.chars().take(8).collect()
}

/// One line, at most `max` characters - gatehouse refuses anything else.
fn line(raw: &str, max: usize) -> String {
    let flat: String = raw
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let flat = flat.trim();
    if flat.chars().count() <= max {
        flat.to_string()
    } else {
        flat.chars().take(max - 1).collect::<String>() + "…"
    }
}

/// Queues the notification for a finished run for everyone who follows it and may still read it.
/// Returns how many were queued. Never fatal to the caller - a run's result must not depend on this.
pub async fn on_run_finished(
    db: &Db,
    notifications: &Notifications,
    repo: &Repo,
    run: &Run,
    status: Status,
) -> Result<usize, QueueError> {
    let Some(template) = event_for(db, run, status).await? else {
        return Ok(0);
    };
    let Some(url) = crate::scheduler::worker::run_url(&run.id) else {
        tracing::warn!("CONVEYOR_PUBLIC_URL is not set: run notifications need a link to send");
        return Ok(0);
    };

    let followers = subscriptions::subscribers(db, &repo.id, &repo.project_id).await?;
    if followers.is_empty() {
        return Ok(0);
    }

    let chain = projects::ancestor_chain(db, &repo.project_id).await?;
    let project = projects::full_path(db, &repo.project_id)
        .await?
        .unwrap_or_else(|| repo.slug());
    let vars: BTreeMap<String, String> = [
        ("project", line(&project, 250)),
        (
            "run",
            line(&format!("{} ({})", short_id(&run.id), run.short_sha()), 250),
        ),
        ("ref", line(run.ref_name(), 250)),
        ("url", url),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect();

    let mut queued = 0;
    for username in followers {
        if notifications.auth_enabled {
            // Access can be taken away after someone subscribed; a stale subscription must not keep
            // leaking what a run was called and how it went.
            let allowed = notifications
                .user_db
                .get_user(&username)
                .await
                .is_some_and(|user| may_read(&user, &chain));
            if !allowed {
                continue;
            }
        }
        if outbox::enqueue(db, &run.id, &username, template, &vars).await? {
            queued += 1;
        }
    }
    Ok(queued)
}

/// Sends what is due once; returns how many messages it settled one way or the other.
pub async fn deliver_due(db: &Db, notifier: &dyn Notifier) -> Result<usize, QueueError> {
    let due = outbox::claim_due(db, 20).await?;
    let count = due.len();

    for message in due {
        match notifier.deliver(&message).await {
            Delivery::Done => outbox::complete(db, &message.id).await?,
            Delivery::Rejected(error) => {
                tracing::error!(
                    "gatehouse refused the {} notification for {}: {error}",
                    message.template,
                    message.username
                );
                outbox::give_up(db, &message.id, &error).await?;
            }
            Delivery::Retry(error) if message.attempts >= outbox::MAX_ATTEMPTS => {
                tracing::error!(
                    "giving up on the {} notification for {} after {} attempts: {error}",
                    message.template,
                    message.username,
                    message.attempts
                );
                outbox::give_up(db, &message.id, &error).await?;
            }
            Delivery::Retry(error) => {
                tracing::warn!(
                    "the {} notification for {} will be retried: {error}",
                    message.template,
                    message.username
                );
                outbox::retry_later(
                    db,
                    &message.id,
                    &error,
                    outbox::backoff_secs(message.attempts),
                )
                .await?;
            }
        }
    }
    Ok(count)
}

/// Starts the loop that keeps delivering; returns immediately.
pub fn spawn_delivery(db: Db, notifier: Arc<dyn Notifier>) {
    tokio::spawn(async move {
        loop {
            match deliver_due(&db, notifier.as_ref()).await {
                Ok(0) => tokio::time::sleep(IDLE_POLL).await,
                Ok(_) => {}
                Err(error) => {
                    tracing::error!("could not deliver notifications: {error}");
                    tokio::time::sleep(IDLE_POLL).await;
                }
            }
        }
    });
}
