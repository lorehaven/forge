//! Messages waiting for gatehouse. A run finishing writes them; a background loop delivers them, so
//! gatehouse or the mail server being down for a while delays a notification rather than losing it.

use crate::scheduler::queue::{QueueError, pool, schema};
use quench_db::prelude::Db;
use sqlx::Row;
use std::collections::BTreeMap;
use uuid::Uuid;

/// Attempts before a message is set aside for a human to look at.
pub const MAX_ATTEMPTS: i32 = 10;

/// How long a claimed message is invisible to other senders - the time a crashed one holds it.
pub const LEASE_SECS: f64 = 120.0;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pending {
    pub id: String,
    pub run_id: String,
    pub username: String,
    pub template: String,
    pub vars: BTreeMap<String, String>,
    /// Including the attempt this claim is about to make.
    pub attempts: i32,
}

/// Queues one message; `false` when this run already queued this kind for this person.
pub async fn enqueue(
    db: &Db,
    run_id: &str,
    username: &str,
    template: &str,
    vars: &BTreeMap<String, String>,
) -> Result<bool, QueueError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!(
        "INSERT INTO {schema}.notification_outbox (id, run_id, username, template, vars) \
         VALUES ($1, $2, $3, $4, $5) ON CONFLICT DO NOTHING"
    );
    let vars = serde_json::to_string(vars).map_err(|e| QueueError::BadRow(e.to_string()))?;

    let done = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(Uuid::new_v4().to_string())
        .bind(run_id)
        .bind(username)
        .bind(template)
        .bind(vars)
        .execute(pool)
        .await?;
    Ok(done.rows_affected() > 0)
}

/// Takes up to `limit` due messages, leasing each so a second sender skips it meanwhile.
pub async fn claim_due(db: &Db, limit: i64) -> Result<Vec<Pending>, QueueError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!(
        "UPDATE {schema}.notification_outbox SET \
             attempts = attempts + 1, \
             next_attempt_at = NOW() + make_interval(secs => $2) \
         WHERE id IN ( \
             SELECT id FROM {schema}.notification_outbox \
             WHERE failed_at IS NULL AND next_attempt_at <= NOW() \
             ORDER BY next_attempt_at LIMIT $1 FOR UPDATE SKIP LOCKED \
         ) RETURNING id, run_id, username, template, vars, attempts"
    );

    let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(limit)
        .bind(LEASE_SECS)
        .fetch_all(pool)
        .await?;

    rows.iter()
        .map(|row| {
            let raw: String = row.try_get("vars")?;
            Ok(Pending {
                id: row.try_get("id")?,
                run_id: row.try_get("run_id")?,
                username: row.try_get("username")?,
                template: row.try_get("template")?,
                vars: serde_json::from_str(&raw).map_err(|e| QueueError::BadRow(e.to_string()))?,
                attempts: row.try_get("attempts")?,
            })
        })
        .collect()
}

/// Gatehouse answered for good - sent, or deliberately not.
pub async fn complete(db: &Db, id: &str) -> Result<(), QueueError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!("DELETE FROM {schema}.notification_outbox WHERE id = $1");
    sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Try again in `delay_secs`.
pub async fn retry_later(
    db: &Db,
    id: &str,
    error: &str,
    delay_secs: f64,
) -> Result<(), QueueError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!(
        "UPDATE {schema}.notification_outbox SET \
             next_attempt_at = NOW() + make_interval(secs => $2), last_error = $3 \
         WHERE id = $1"
    );
    sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(id)
        .bind(delay_secs)
        .bind(error)
        .execute(pool)
        .await?;
    Ok(())
}

/// Stops trying; the row stays, with why, for someone to read.
pub async fn give_up(db: &Db, id: &str, error: &str) -> Result<(), QueueError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!(
        "UPDATE {schema}.notification_outbox SET failed_at = NOW(), last_error = $2 WHERE id = $1"
    );
    sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(id)
        .bind(error)
        .execute(pool)
        .await?;
    Ok(())
}

/// Seconds to wait after the `attempts`-th failure: 30s, 1m, 2m ... capped at an hour.
pub fn backoff_secs(attempts: i32) -> f64 {
    let exponent = (attempts.max(1) - 1).min(7) as u32;
    (30.0 * f64::from(2u32.pow(exponent))).min(3600.0)
}
