//! Folders of a mailbox.

use super::Store;
use crate::error::{Result, new_id};

#[derive(Debug, Clone)]
pub struct FolderRow {
    pub id: String,
    pub account_id: String,
    pub path: String,
    pub name: String,
    pub role: String,
    pub uid_validity: i64,
    pub uid_next: i64,
    pub unread: i32,
    pub total: i32,
}

fn from_row(row: &quench_pg::Row) -> Result<FolderRow> {
    Ok(FolderRow {
        id: row.get("id")?,
        account_id: row.get("account_id")?,
        path: row.get("path")?,
        name: row.get("name")?,
        role: row.get("role")?,
        uid_validity: row.get("uid_validity")?,
        uid_next: row.get("uid_next")?,
        unread: row.get("unread")?,
        total: row.get("total")?,
    })
}

const COLUMNS: &str = "id, account_id, path, name, role, uid_validity, uid_next, unread, total";

impl Store {
    /// Makes sure the folder exists (updating its name and role) and returns it.
    pub async fn folder_upsert(
        &self,
        account_id: &str,
        path: &str,
        name: &str,
        role: &str,
    ) -> Result<FolderRow> {
        let id = new_id();
        let row = self
            .pool
            .query_one(
                &format!(
                    "INSERT INTO folders (id, account_id, path, name, role) VALUES ($1,$2,$3,$4,$5) \
                     ON CONFLICT (account_id, path) DO UPDATE SET name = EXCLUDED.name, role = EXCLUDED.role \
                     RETURNING {COLUMNS}"
                ),
                &[&id, &account_id, &path, &name, &role],
            )
            .await?;
        from_row(&row)
    }

    pub async fn folders_of(&self, account_id: &str) -> Result<Vec<FolderRow>> {
        let rows = self
            .pool
            .query(
                &format!("SELECT {COLUMNS} FROM folders WHERE account_id = $1 ORDER BY CASE role WHEN 'inbox' THEN 0 WHEN 'drafts' THEN 1 WHEN 'sent' THEN 2 WHEN 'archive' THEN 3 WHEN 'junk' THEN 4 WHEN 'trash' THEN 5 ELSE 6 END, path"),
                &[&account_id],
            )
            .await?;
        rows.iter().map(from_row).collect()
    }

    pub async fn folder_get(&self, id: &str) -> Result<Option<FolderRow>> {
        let row = self
            .pool
            .query_opt(
                &format!("SELECT {COLUMNS} FROM folders WHERE id = $1"),
                &[&id],
            )
            .await?;
        row.as_ref().map(from_row).transpose()
    }

    /// A folder by its role in a mailbox (the first, when a server has several).
    pub async fn folder_by_role(&self, account_id: &str, role: &str) -> Result<Option<FolderRow>> {
        let row = self
            .pool
            .query_opt(
                &format!("SELECT {COLUMNS} FROM folders WHERE account_id = $1 AND role = $2 ORDER BY path LIMIT 1"),
                &[&account_id, &role],
            )
            .await?;
        row.as_ref().map(from_row).transpose()
    }

    /// Forgets folders the server no longer lists.
    pub async fn folders_keep(&self, account_id: &str, keep: &[String]) -> Result<()> {
        let keep_list = quench_pg::PgArray(keep.to_vec());
        self.pool
            .execute(
                "DELETE FROM folders WHERE account_id = $1 AND NOT (path = ANY($2::text[]))",
                &[&account_id, &keep_list],
            )
            .await?;
        Ok(())
    }

    /// Records the numbering the server uses.
    pub async fn folder_set_sync(&self, id: &str, uid_validity: i64, uid_next: i64) -> Result<()> {
        self.pool
            .execute(
                "UPDATE folders SET uid_validity = $2, uid_next = $3 WHERE id = $1",
                &[&id, &uid_validity, &uid_next],
            )
            .await?;
        Ok(())
    }

    /// Recounts unread and total from the cached messages.
    pub async fn folder_recount(&self, id: &str) -> Result<()> {
        self.pool
            .execute(
                "UPDATE folders SET total = (SELECT COUNT(*) FROM messages WHERE folder_id = $1), \
                 unread = (SELECT COUNT(*) FROM messages WHERE folder_id = $1 AND NOT seen) WHERE id = $1",
                &[&id],
            )
            .await?;
        Ok(())
    }
}
