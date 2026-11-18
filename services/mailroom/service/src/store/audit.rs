//! What administrators did.

use super::Store;
use crate::error::{Result, new_id, now};
use mailroom_core::admin::AuditEntry;

impl Store {
    /// Writes a line. `detail` must not hold secrets (it is shown to administrators).
    pub async fn audit(&self, actor: &str, action: &str, target: &str, detail: &str) -> Result<()> {
        let detail: String = detail.chars().take(500).collect();
        self.pool
            .execute(
                "INSERT INTO audit (id, at, actor, action, target, detail) VALUES ($1,$2,$3,$4,$5,$6)",
                &[&new_id(), &now(), &actor, &action, &target, &detail],
            )
            .await?;
        Ok(())
    }

    pub async fn audit_list(&self, offset: i64, limit: i64) -> Result<(Vec<AuditEntry>, i64)> {
        let total = self
            .pool
            .query_one("SELECT COUNT(*) AS n FROM audit", &[])
            .await?;
        let rows = self
            .pool
            .query(
                "SELECT id, at, actor, action, target, detail FROM audit ORDER BY at DESC, id OFFSET $1 LIMIT $2",
                &[&offset, &limit],
            )
            .await?;
        let items = rows
            .iter()
            .map(|r| {
                Ok(AuditEntry {
                    id: r.get("id")?,
                    at: r.get("at")?,
                    actor: r.get("actor")?,
                    action: r.get("action")?,
                    target: r.get("target")?,
                    detail: r.get("detail")?,
                })
            })
            .collect::<Result<_>>()?;
        Ok((items, total.get("n")?))
    }
}
