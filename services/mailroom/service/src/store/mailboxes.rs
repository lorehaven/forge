//! One mailbox per person.

use super::Store;
use crate::error::Result;

#[derive(Debug, Clone)]
pub struct MailboxRow {
    pub owner: String,
    pub address: String,
    pub created: i64,
    pub last_seen: Option<i64>,
    pub disabled: bool,
}

/// A mailbox with what it holds, for the administrator.
#[derive(Debug, Clone)]
pub struct MailboxUsage {
    pub mailbox: MailboxRow,
    pub messages: i64,
    pub bytes: i64,
}

fn from_row(row: &quench_pg::Row) -> Result<MailboxRow> {
    Ok(MailboxRow {
        owner: row.get("owner")?,
        address: row.get("address")?,
        created: row.get("created")?,
        last_seen: row.get("last_seen")?,
        disabled: row.get("disabled")?,
    })
}

impl Store {
    pub async fn mailbox_get(&self, owner: &str) -> Result<Option<MailboxRow>> {
        let row = self
            .pool
            .query_opt(
                "SELECT owner, address, created, last_seen, disabled FROM mailboxes WHERE owner = $1",
                &[&owner],
            )
            .await?;
        row.as_ref().map(from_row).transpose()
    }

    /// The mailbox of `owner`, made when there is none yet.
    pub async fn mailbox_ensure(&self, owner: &str, address: &str, now: i64) -> Result<MailboxRow> {
        self.pool
            .execute(
                "INSERT INTO mailboxes (owner, address, created) VALUES ($1, $2, $3) ON CONFLICT (owner) DO NOTHING",
                &[&owner, &address, &now],
            )
            .await?;
        let row = self
            .pool
            .query_one(
                "SELECT owner, address, created, last_seen, disabled FROM mailboxes WHERE owner = $1",
                &[&owner],
            )
            .await?;
        from_row(&row)
    }

    pub async fn mailbox_touch(&self, owner: &str, now: i64) -> Result<()> {
        self.pool
            .execute(
                "UPDATE mailboxes SET last_seen = $2 WHERE owner = $1",
                &[&owner, &now],
            )
            .await?;
        Ok(())
    }

    pub async fn mailbox_count(&self) -> Result<i64> {
        let row = self
            .pool
            .query_one("SELECT COUNT(*) AS n FROM mailboxes", &[])
            .await?;
        Ok(row.get("n")?)
    }

    /// Addresses that start with `prefix` (already lower-cased, LIKE-escaped by the caller via [`like_prefix`]), by name.
    pub async fn mailbox_search(&self, prefix: &str, limit: i64) -> Result<Vec<String>> {
        let rows = self
            .pool
            .query(
                "SELECT address FROM mailboxes WHERE owner LIKE $1 ORDER BY owner LIMIT $2",
                &[&like_prefix(prefix), &limit],
            )
            .await?;
        rows.iter().map(|r| Ok(r.get("address")?)).collect()
    }

    pub async fn mailboxes_usage(
        &self,
        offset: i64,
        limit: i64,
    ) -> Result<(Vec<MailboxUsage>, i64)> {
        let rows = self
            .pool
            .query(
                "SELECT mb.owner, mb.address, mb.created, mb.last_seen, mb.disabled, \
                        COUNT(m.id)::BIGINT AS messages, COALESCE(SUM(m.size), 0)::BIGINT AS bytes \
                 FROM mailboxes mb LEFT JOIN messages m ON m.owner = mb.owner \
                 GROUP BY mb.owner ORDER BY mb.owner OFFSET $1 LIMIT $2",
                &[&offset, &limit],
            )
            .await?;
        let items = rows
            .iter()
            .map(|r| {
                Ok(MailboxUsage {
                    mailbox: from_row(r)?,
                    messages: r.get("messages")?,
                    bytes: r.get("bytes")?,
                })
            })
            .collect::<Result<_>>()?;
        Ok((items, self.mailbox_count().await?))
    }

    pub async fn mailbox_set_disabled(&self, owner: &str, disabled: bool) -> Result<bool> {
        Ok(self
            .pool
            .execute(
                "UPDATE mailboxes SET disabled = $2 WHERE owner = $1",
                &[&owner, &disabled],
            )
            .await?
            > 0)
    }

    /// Removes the mailbox with every message in it (and the person's uploads).
    pub async fn mailbox_delete(&self, owner: &str) -> Result<bool> {
        self.pool
            .execute("DELETE FROM uploads WHERE owner = $1", &[&owner])
            .await?;
        Ok(self
            .pool
            .execute("DELETE FROM mailboxes WHERE owner = $1", &[&owner])
            .await?
            > 0)
    }
}

/// `word%` with LIKE's own characters escaped.
#[must_use]
pub fn like_prefix(word: &str) -> String {
    let mut out = String::new();
    for c in word.chars() {
        if matches!(c, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('%');
    out
}
