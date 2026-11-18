//! What the services delivered.

use super::Store;
use crate::error::{Result, is_unique_violation};
use mailroom_core::admin::Delivery;

#[derive(Debug, Clone)]
pub struct DeliveryRow {
    pub id: String,
    pub source: String,
    pub kind: String,
    pub recipients: Vec<String>,
    pub subject: String,
    pub status: String,
    pub delivered: i32,
    pub error: Option<String>,
    pub created: i64,
    pub idem_key: Option<String>,
}

impl Store {
    /// Records that a delivery started. `false` when the same source already used this idempotency key.
    pub async fn delivery_begin(&self, d: &DeliveryRow) -> Result<bool> {
        let result = self
            .pool
            .execute(
                "INSERT INTO deliveries (id, source, kind, recipients, subject, status, delivered, error, created, idem_key) \
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
                &[
                    &d.id,
                    &d.source,
                    &d.kind,
                    &quench_serde::json::to_string(&d.recipients),
                    &d.subject,
                    &d.status,
                    &d.delivered,
                    &d.error,
                    &d.created,
                    &d.idem_key,
                ],
            )
            .await;
        match result {
            Ok(_) => Ok(true),
            Err(e) if is_unique_violation(&e) => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    pub async fn delivery_finish(
        &self,
        id: &str,
        status: &str,
        delivered: i32,
        error: Option<&str>,
    ) -> Result<()> {
        self.pool
            .execute(
                "UPDATE deliveries SET status = $2, delivered = $3, error = $4 WHERE id = $1",
                &[&id, &status, &delivered, &error],
            )
            .await?;
        Ok(())
    }

    /// Frees the idempotency key of a delivery that failed, so a retry is not taken for a duplicate.
    pub async fn delivery_release_key(&self, id: &str) -> Result<()> {
        self.pool
            .execute(
                "UPDATE deliveries SET idem_key = NULL WHERE id = $1",
                &[&id],
            )
            .await?;
        Ok(())
    }

    /// The delivery that used `key`: its id and how many mailboxes it reached.
    pub async fn delivery_by_key(&self, source: &str, key: &str) -> Result<Option<(String, i32)>> {
        let row = self
            .pool
            .query_opt(
                "SELECT id, delivered FROM deliveries WHERE source = $1 AND idem_key = $2",
                &[&source, &key],
            )
            .await?;
        row.map(|r| Ok((r.get("id")?, r.get("delivered")?)))
            .transpose()
    }

    pub async fn deliveries_list(&self, offset: i64, limit: i64) -> Result<(Vec<Delivery>, i64)> {
        let total = self
            .pool
            .query_one("SELECT COUNT(*)::BIGINT AS n FROM deliveries", &[])
            .await?;
        let rows = self
            .pool
            .query(
                "SELECT id, source, kind, recipients, subject, status, delivered, error, created \
                 FROM deliveries ORDER BY created DESC, seq DESC OFFSET $1 LIMIT $2",
                &[&offset, &limit],
            )
            .await?;
        let items = rows
            .iter()
            .map(|r| {
                let recipients: String = r.get("recipients")?;
                let delivered: i32 = r.get("delivered")?;
                Ok(Delivery {
                    id: r.get("id")?,
                    source: r.get("source")?,
                    kind: r.get("kind")?,
                    recipients: quench_serde::json::from_str(&recipients).unwrap_or_default(),
                    subject: r.get("subject")?,
                    status: r.get("status")?,
                    delivered: u32::try_from(delivered).unwrap_or(0),
                    error: r.get("error")?,
                    created: r.get("created")?,
                })
            })
            .collect::<Result<_>>()?;
        Ok((items, total.get("n")?))
    }

    /// `(mailboxes reached, failed deliveries)` since `since`.
    pub async fn delivery_counts(&self, since: i64) -> Result<(i64, i64)> {
        let row = self
            .pool
            .query_one(
                "SELECT COALESCE(SUM(delivered), 0)::BIGINT AS ok, \
                        COUNT(*) FILTER (WHERE status IN ('failed', 'partial'))::BIGINT AS failed \
                 FROM deliveries WHERE created >= $1",
                &[&since],
            )
            .await?;
        Ok((row.get("ok")?, row.get("failed")?))
    }

    pub async fn deliveries_prune(&self, before: i64) -> Result<u64> {
        Ok(self
            .pool
            .execute("DELETE FROM deliveries WHERE created < $1", &[&before])
            .await?)
    }
}
