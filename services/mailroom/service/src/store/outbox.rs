//! The queue of mail to send, and its history.

use super::Store;
use crate::error::{Result, is_unique_violation};

#[derive(Debug, Clone)]
pub struct OutboxRow {
    pub id: String,
    pub kind: String,
    pub source: String,
    pub account_id: Option<String>,
    pub envelope_from: String,
    /// JSON array of addresses.
    pub recipients: String,
    pub subject: String,
    pub raw: Option<Vec<u8>>,
    pub status: String,
    pub attempts: i32,
    pub last_error: Option<String>,
    pub created: i64,
    pub next_attempt: Option<i64>,
    pub sent: Option<i64>,
    pub idem_key: Option<String>,
    pub reply_to_message: Option<String>,
}

const COLUMNS: &str = "id, kind, source, account_id, envelope_from, recipients, subject, raw, status, attempts, last_error, \
                       created, next_attempt, sent, idem_key, reply_to_message";

fn from_row(row: &quench_pg::Row) -> Result<OutboxRow> {
    Ok(OutboxRow {
        id: row.get("id")?,
        kind: row.get("kind")?,
        source: row.get("source")?,
        account_id: row.get("account_id")?,
        envelope_from: row.get("envelope_from")?,
        recipients: row.get("recipients")?,
        subject: row.get("subject")?,
        raw: row.get("raw")?,
        status: row.get("status")?,
        attempts: row.get("attempts")?,
        last_error: row.get("last_error")?,
        created: row.get("created")?,
        next_attempt: row.get("next_attempt")?,
        sent: row.get("sent")?,
        idem_key: row.get("idem_key")?,
        reply_to_message: row.get("reply_to_message")?,
    })
}

impl Store {
    /// Queues a message. `Ok(false)` when the idempotency key was already used by this source.
    pub async fn outbox_insert(&self, o: &OutboxRow) -> Result<bool> {
        let result = self
            .pool
            .execute(
                "INSERT INTO outbox (id, kind, source, account_id, envelope_from, recipients, subject, raw, status, attempts, \
                 last_error, created, next_attempt, sent, idem_key, reply_to_message) \
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16)",
                &[
                    &o.id, &o.kind, &o.source, &o.account_id, &o.envelope_from, &o.recipients, &o.subject, &o.raw,
                    &o.status, &o.attempts, &o.last_error, &o.created, &o.next_attempt, &o.sent, &o.idem_key,
                    &o.reply_to_message,
                ],
            )
            .await;
        match result {
            Ok(_) => Ok(true),
            Err(e) if is_unique_violation(&e) => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    /// The id of the queued message with this key, if there is one (the answer to a retry).
    pub async fn outbox_by_key(&self, source: &str, key: &str) -> Result<Option<String>> {
        let row = self
            .pool
            .query_opt(
                "SELECT id FROM outbox WHERE source = $1 AND idem_key = $2",
                &[&source, &key],
            )
            .await?;
        row.map(|r| Ok(r.get("id")?)).transpose()
    }

    pub async fn outbox_get(&self, id: &str) -> Result<Option<OutboxRow>> {
        let row = self
            .pool
            .query_opt(
                &format!("SELECT {COLUMNS} FROM outbox WHERE id = $1"),
                &[&id],
            )
            .await?;
        row.as_ref().map(from_row).transpose()
    }

    /// Takes up to `limit` messages that are due, marking them `sending`. Safe against two workers.
    pub async fn outbox_claim(&self, now: i64, limit: i64) -> Result<Vec<OutboxRow>> {
        let rows = self
            .pool
            .query(
                &format!(
                    "UPDATE outbox SET status = 'sending' WHERE id IN (\
                       SELECT id FROM outbox WHERE status = 'queued' AND COALESCE(next_attempt, 0) <= $1 \
                       ORDER BY created LIMIT $2 FOR UPDATE SKIP LOCKED) RETURNING {COLUMNS}"
                ),
                &[&now, &limit],
            )
            .await?;
        rows.iter().map(from_row).collect()
    }

    /// Puts messages left `sending` by a crashed worker back in the queue.
    pub async fn outbox_requeue_stuck(&self, before: i64) -> Result<u64> {
        Ok(self
            .pool
            .execute(
                "UPDATE outbox SET status = 'queued' WHERE status = 'sending' AND COALESCE(next_attempt, created) < $1",
                &[&before],
            )
            .await?)
    }

    /// Marks a message sent and drops its body.
    pub async fn outbox_mark_sent(&self, id: &str, now: i64) -> Result<()> {
        self.pool
            .execute(
                "UPDATE outbox SET status='sent', sent=$2, raw=NULL, last_error=NULL, next_attempt=NULL WHERE id=$1",
                &[&id, &now],
            )
            .await?;
        Ok(())
    }

    /// Back to the queue for another try.
    pub async fn outbox_mark_retry(
        &self,
        id: &str,
        attempts: i32,
        next: i64,
        error: &str,
    ) -> Result<()> {
        self.pool
            .execute(
                "UPDATE outbox SET status='queued', attempts=$2, next_attempt=$3, last_error=$4 WHERE id=$1",
                &[&id, &attempts, &next, &error],
            )
            .await?;
        Ok(())
    }

    /// Gives up (and drops the body: a failed message is not kept to be read).
    pub async fn outbox_mark_failed(&self, id: &str, attempts: i32, error: &str) -> Result<()> {
        self.pool
            .execute(
                "UPDATE outbox SET status='failed', attempts=$2, last_error=$3, raw=NULL, next_attempt=NULL WHERE id=$1",
                &[&id, &attempts, &error],
            )
            .await?;
        Ok(())
    }

    /// History and queue, newest first, with an optional status.
    pub async fn outbox_list(
        &self,
        status: Option<&str>,
        offset: i64,
        limit: i64,
    ) -> Result<(Vec<OutboxRow>, i64)> {
        let status = status.unwrap_or("");
        let count = self
            .pool
            .query_one(
                "SELECT COUNT(*) AS n FROM outbox WHERE ($1 = '' OR status = $1)",
                &[&status],
            )
            .await?;
        let rows = self
            .pool
            .query(
                &format!(
                    "SELECT {COLUMNS} FROM outbox WHERE ($1 = '' OR status = $1) ORDER BY created DESC OFFSET $2 LIMIT $3"
                ),
                &[&status, &offset, &limit],
            )
            .await?;
        Ok((
            rows.iter().map(from_row).collect::<Result<_>>()?,
            count.get("n")?,
        ))
    }

    /// Messages created since `since` by one source, or by everyone when `source` is empty (budgets).
    pub async fn outbox_count_since(&self, source: &str, kind: &str, since: i64) -> Result<i64> {
        let row = self
            .pool
            .query_one(
                "SELECT COUNT(*) AS n FROM outbox WHERE kind = $2 AND created >= $3 AND ($1 = '' OR source = $1)",
                &[&source, &kind, &since],
            )
            .await?;
        Ok(row.get("n")?)
    }

    /// Counts for the panel: queued now, sent and failed since `since`.
    pub async fn outbox_stats(&self, since: i64) -> Result<(i64, i64, i64)> {
        let row = self
            .pool
            .query_one(
                "SELECT COUNT(*) FILTER (WHERE status IN ('queued','sending')) AS queued, \
                 COUNT(*) FILTER (WHERE status = 'sent' AND sent >= $1) AS sent, \
                 COUNT(*) FILTER (WHERE status = 'failed' AND created >= $1) AS failed FROM outbox",
                &[&since],
            )
            .await?;
        Ok((row.get("queued")?, row.get("sent")?, row.get("failed")?))
    }

    /// A failed message goes back to the queue (it has no body any more: only for the retry of a transient failure kept in
    /// the queue; a failed one is re-queued by the person sending it again). Returns whether a row was waiting.
    pub async fn outbox_retry_now(&self, id: &str, now: i64) -> Result<bool> {
        let n = self
            .pool
            .execute(
                "UPDATE outbox SET status='queued', next_attempt=$2 WHERE id=$1 AND status='queued'",
                &[&id, &now],
            )
            .await?;
        Ok(n > 0)
    }

    /// Removes a queue entry that has not been sent (cancel), or a history row.
    pub async fn outbox_delete(&self, id: &str) -> Result<bool> {
        Ok(self
            .pool
            .execute(
                "DELETE FROM outbox WHERE id = $1 AND status <> 'sending'",
                &[&id],
            )
            .await?
            > 0)
    }

    /// Drops history older than `before`.
    pub async fn outbox_prune(&self, before: i64) -> Result<u64> {
        Ok(self
            .pool
            .execute(
                "DELETE FROM outbox WHERE status IN ('sent','failed') AND created < $1",
                &[&before],
            )
            .await?)
    }
}
