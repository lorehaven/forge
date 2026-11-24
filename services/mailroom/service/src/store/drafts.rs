//! Messages being written, and the senders a person muted.

use super::Store;
use crate::error::Result;

#[derive(Debug, Clone)]
pub struct DraftRow {
    pub id: String,
    pub owner: String,
    pub to_text: String,
    pub cc_text: String,
    pub bcc_text: String,
    pub subject: String,
    pub text: String,
    pub in_reply_to: Option<String>,
    pub updated: i64,
}

const COLUMNS: &str = "id, owner, to_text, cc_text, bcc_text, subject, text, in_reply_to, updated";

fn from_row(row: &quench_pg::Row) -> Result<DraftRow> {
    Ok(DraftRow {
        id: row.get("id")?,
        owner: row.get("owner")?,
        to_text: row.get("to_text")?,
        cc_text: row.get("cc_text")?,
        bcc_text: row.get("bcc_text")?,
        subject: row.get("subject")?,
        text: row.get("text")?,
        in_reply_to: row.get("in_reply_to")?,
        updated: row.get("updated")?,
    })
}

impl Store {
    /// The owner's drafts, most recently changed first.
    pub async fn drafts_of(&self, owner: &str) -> Result<Vec<DraftRow>> {
        let rows = self
            .pool
            .query(
                &format!(
                    "SELECT {COLUMNS} FROM drafts WHERE owner = $1 ORDER BY updated DESC LIMIT 200"
                ),
                &[&owner],
            )
            .await?;
        rows.iter().map(from_row).collect()
    }

    pub async fn draft_count(&self, owner: &str) -> Result<i64> {
        let row = self
            .pool
            .query_one(
                "SELECT COUNT(*)::BIGINT AS n FROM drafts WHERE owner = $1",
                &[&owner],
            )
            .await?;
        Ok(row.get("n")?)
    }

    pub async fn draft_get(&self, id: &str, owner: &str) -> Result<Option<DraftRow>> {
        let row = self
            .pool
            .query_opt(
                &format!("SELECT {COLUMNS} FROM drafts WHERE id = $1 AND owner = $2"),
                &[&id, &owner],
            )
            .await?;
        row.as_ref().map(from_row).transpose()
    }

    /// Inserts the draft, or changes the owner's draft of that id. Another person's id is left alone (returns `false`).
    pub async fn draft_put(&self, d: &DraftRow) -> Result<bool> {
        let n = self
            .pool
            .execute(
                "INSERT INTO drafts (id, owner, to_text, cc_text, bcc_text, subject, text, in_reply_to, updated) \
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) \
                 ON CONFLICT (id) DO UPDATE SET to_text = $3, cc_text = $4, bcc_text = $5, subject = $6, text = $7, \
                 in_reply_to = $8, updated = $9 WHERE drafts.owner = $2",
                &[&d.id, &d.owner, &d.to_text, &d.cc_text, &d.bcc_text, &d.subject, &d.text, &d.in_reply_to, &d.updated],
            )
            .await?;
        Ok(n > 0)
    }

    pub async fn draft_delete(&self, id: &str, owner: &str) -> Result<bool> {
        let n = self
            .pool
            .execute(
                "DELETE FROM drafts WHERE id = $1 AND owner = $2",
                &[&id, &owner],
            )
            .await?;
        Ok(n > 0)
    }

    pub async fn mutes_of(&self, owner: &str) -> Result<Vec<String>> {
        let rows = self
            .pool
            .query(
                "SELECT address FROM mutes WHERE owner = $1 ORDER BY address",
                &[&owner],
            )
            .await?;
        rows.iter().map(|r| Ok(r.get("address")?)).collect()
    }

    /// Mutes `address` unless the owner already has `cap` of them. Returns whether it is muted now.
    pub async fn mute_add(&self, owner: &str, address: &str, now: i64, cap: i64) -> Result<bool> {
        let mut conn = self.pool.get().await?;
        let mut tx = conn.begin().await?;
        tx.execute("SELECT pg_advisory_xact_lock(hashtext($1))", &[&owner])
            .await?;
        let n = tx
            .execute(
                "INSERT INTO mutes (owner, address, created) SELECT $1,$2,$3 \
                 WHERE (SELECT COUNT(*) FROM mutes WHERE owner = $1) < $4::bigint \
                 ON CONFLICT DO NOTHING",
                &[&owner, &address, &now, &cap],
            )
            .await?;
        let there = tx
            .query_opt(
                "SELECT 1 AS one FROM mutes WHERE owner = $1 AND address = $2",
                &[&owner, &address],
            )
            .await?
            .is_some();
        tx.commit().await?;
        let _ = n;
        Ok(there)
    }

    pub async fn mute_remove(&self, owner: &str, address: &str) -> Result<()> {
        self.pool
            .execute(
                "DELETE FROM mutes WHERE owner = $1 AND address = $2",
                &[&owner, &address],
            )
            .await?;
        Ok(())
    }

    /// Which of `owners` muted `address`.
    pub async fn muted_among(&self, owners: &[String], address: &str) -> Result<Vec<String>> {
        let mut out = Vec::new();
        for owner in owners {
            let row = self
                .pool
                .query_opt(
                    "SELECT 1 AS one FROM mutes WHERE owner = $1 AND address = $2",
                    &[owner, &address],
                )
                .await?;
            if row.is_some() {
                out.push(owner.clone());
            }
        }
        Ok(out)
    }
}

impl Store {
    pub async fn signature_get(&self, owner: &str) -> Result<String> {
        let row = self
            .pool
            .query_opt("SELECT text FROM signatures WHERE owner = $1", &[&owner])
            .await?;
        row.map_or_else(|| Ok(String::new()), |r| Ok(r.get("text")?))
    }

    pub async fn signature_put(&self, owner: &str, text: &str) -> Result<()> {
        self.pool
            .execute(
                "INSERT INTO signatures (owner, text) VALUES ($1, $2) ON CONFLICT (owner) DO UPDATE SET text = $2",
                &[&owner, &text],
            )
            .await?;
        Ok(())
    }
}
