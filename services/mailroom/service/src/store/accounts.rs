//! Mailboxes people added.

use super::Store;
use crate::error::Result;

/// One row.
#[derive(Debug, Clone)]
pub struct AccountRow {
    pub id: String,
    pub owner: String,
    pub label: String,
    pub address: String,
    pub username: String,
    pub imap_host: String,
    pub imap_port: i32,
    pub imap_security: String,
    pub smtp_host: String,
    pub smtp_port: i32,
    pub smtp_security: String,
    /// Sealed by the vault.
    pub secret: String,
    pub tls_roots: String,
    pub signature: String,
    pub save_sent: bool,
    pub state: String,
    pub last_error: Option<String>,
    pub last_sync: Option<i64>,
    pub disabled: bool,
    pub created: i64,
}

const COLUMNS: &str = "id, owner, label, address, username, imap_host, imap_port, imap_security, smtp_host, smtp_port, \
                       smtp_security, secret, tls_roots, signature, save_sent, state, last_error, last_sync, disabled, created";

fn from_row(row: &quench_pg::Row) -> Result<AccountRow> {
    Ok(AccountRow {
        id: row.get("id")?,
        owner: row.get("owner")?,
        label: row.get("label")?,
        address: row.get("address")?,
        username: row.get("username")?,
        imap_host: row.get("imap_host")?,
        imap_port: row.get("imap_port")?,
        imap_security: row.get("imap_security")?,
        smtp_host: row.get("smtp_host")?,
        smtp_port: row.get("smtp_port")?,
        smtp_security: row.get("smtp_security")?,
        secret: row.get("secret")?,
        tls_roots: row.get("tls_roots")?,
        signature: row.get("signature")?,
        save_sent: row.get("save_sent")?,
        state: row.get("state")?,
        last_error: row.get("last_error")?,
        last_sync: row.get("last_sync")?,
        disabled: row.get("disabled")?,
        created: row.get("created")?,
    })
}

impl Store {
    /// Adds a mailbox.
    pub async fn account_insert(&self, a: &AccountRow) -> Result<()> {
        self.pool
            .execute(
                "INSERT INTO accounts (id, owner, label, address, username, imap_host, imap_port, imap_security, \
                 smtp_host, smtp_port, smtp_security, secret, tls_roots, signature, save_sent, state, last_error, \
                 last_sync, disabled, created) \
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20)",
                &[
                    &a.id, &a.owner, &a.label, &a.address, &a.username, &a.imap_host, &a.imap_port,
                    &a.imap_security, &a.smtp_host, &a.smtp_port, &a.smtp_security, &a.secret, &a.tls_roots,
                    &a.signature, &a.save_sent, &a.state, &a.last_error, &a.last_sync, &a.disabled, &a.created,
                ],
            )
            .await?;
        Ok(())
    }

    /// Replaces what a person can edit.
    pub async fn account_update(&self, a: &AccountRow) -> Result<bool> {
        let n = self
            .pool
            .execute(
                "UPDATE accounts SET label=$2, address=$3, username=$4, imap_host=$5, imap_port=$6, imap_security=$7, \
                 smtp_host=$8, smtp_port=$9, smtp_security=$10, secret=$11, tls_roots=$12, signature=$13, save_sent=$14, \
                 state='ok', last_error=NULL WHERE id=$1",
                &[
                    &a.id, &a.label, &a.address, &a.username, &a.imap_host, &a.imap_port, &a.imap_security,
                    &a.smtp_host, &a.smtp_port, &a.smtp_security, &a.secret, &a.tls_roots, &a.signature, &a.save_sent,
                ],
            )
            .await?;
        Ok(n > 0)
    }

    /// One mailbox by id.
    pub async fn account_get(&self, id: &str) -> Result<Option<AccountRow>> {
        let row = self
            .pool
            .query_opt(
                &format!("SELECT {COLUMNS} FROM accounts WHERE id = $1"),
                &[&id],
            )
            .await?;
        row.as_ref().map(from_row).transpose()
    }

    /// One mailbox by id, if `owner` owns it.
    pub async fn account_get_owned(&self, id: &str, owner: &str) -> Result<Option<AccountRow>> {
        let row = self
            .pool
            .query_opt(
                &format!("SELECT {COLUMNS} FROM accounts WHERE id = $1 AND owner = $2"),
                &[&id, &owner],
            )
            .await?;
        row.as_ref().map(from_row).transpose()
    }

    /// A person's mailboxes, oldest first.
    pub async fn accounts_of(&self, owner: &str) -> Result<Vec<AccountRow>> {
        let rows = self
            .pool
            .query(
                &format!("SELECT {COLUMNS} FROM accounts WHERE owner = $1 ORDER BY created, id"),
                &[&owner],
            )
            .await?;
        rows.iter().map(from_row).collect()
    }

    /// Every mailbox (the sync worker, the administrator).
    pub async fn accounts_all(&self, offset: i64, limit: i64) -> Result<Vec<AccountRow>> {
        let rows = self
            .pool
            .query(
                &format!("SELECT {COLUMNS} FROM accounts ORDER BY created, id OFFSET $1 LIMIT $2"),
                &[&offset, &limit],
            )
            .await?;
        rows.iter().map(from_row).collect()
    }

    /// How many mailboxes a person has.
    pub async fn accounts_count_of(&self, owner: &str) -> Result<i64> {
        let row = self
            .pool
            .query_one(
                "SELECT COUNT(*) AS n FROM accounts WHERE owner = $1",
                &[&owner],
            )
            .await?;
        Ok(row.get("n")?)
    }

    /// How many mailboxes there are, and how many are failing.
    pub async fn accounts_counts(&self) -> Result<(i64, i64)> {
        let row = self
            .pool
            .query_one(
                "SELECT COUNT(*) AS n, COUNT(*) FILTER (WHERE state = 'error') AS failing FROM accounts",
                &[],
            )
            .await?;
        Ok((row.get("n")?, row.get("failing")?))
    }

    /// Records how the last sync went.
    pub async fn account_set_state(
        &self,
        id: &str,
        state: &str,
        error: Option<&str>,
        synced: Option<i64>,
    ) -> Result<()> {
        self.pool
            .execute(
                "UPDATE accounts SET state = $2, last_error = $3, last_sync = COALESCE($4, last_sync) WHERE id = $1",
                &[&id, &state, &error, &synced],
            )
            .await?;
        Ok(())
    }

    /// Re-seals the password (after a key rotation).
    pub async fn account_set_secret(&self, id: &str, secret: &str) -> Result<()> {
        self.pool
            .execute(
                "UPDATE accounts SET secret = $2 WHERE id = $1",
                &[&id, &secret],
            )
            .await?;
        Ok(())
    }

    /// The administrator's switch.
    pub async fn account_set_disabled(&self, id: &str, disabled: bool) -> Result<bool> {
        let state = if disabled { "disabled" } else { "ok" };
        let n = self
            .pool
            .execute(
                "UPDATE accounts SET disabled = $2, state = $3, last_error = NULL WHERE id = $1",
                &[&id, &disabled, &state],
            )
            .await?;
        Ok(n > 0)
    }

    /// Removes a mailbox and, by cascade, everything cached from it.
    pub async fn account_delete(&self, id: &str) -> Result<bool> {
        let n = self
            .pool
            .execute("DELETE FROM accounts WHERE id = $1", &[&id])
            .await?;
        Ok(n > 0)
    }

    /// Unread messages across a mailbox's folders (not in junk or trash).
    pub async fn account_unread(&self, id: &str) -> Result<i64> {
        let row = self
            .pool
            .query_one(
                "SELECT COALESCE(SUM(unread), 0)::BIGINT AS n FROM folders WHERE account_id = $1 AND role NOT IN ('junk','trash')",
                &[&id],
            )
            .await?;
        Ok(row.get("n")?)
    }

    /// Cached messages of a mailbox.
    pub async fn account_message_count(&self, id: &str) -> Result<i64> {
        let row = self
            .pool
            .query_one(
                "SELECT COUNT(*) AS n FROM messages WHERE account_id = $1",
                &[&id],
            )
            .await?;
        Ok(row.get("n")?)
    }
}
