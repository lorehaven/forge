//! Files uploaded for a message being written.

use super::Store;
use crate::error::Result;

#[derive(Debug, Clone)]
pub struct UploadRow {
    pub id: String,
    pub owner: String,
    pub filename: String,
    pub media_type: String,
    pub data: Vec<u8>,
    pub size: i32,
    pub created: i64,
}

impl Store {
    /// Stores the file unless the owner's files would then weigh more than `cap` bytes; the check and the insert are one
    /// statement, so parallel uploads cannot each pass it. Returns whether it was stored.
    pub async fn upload_insert(&self, u: &UploadRow, cap: i64) -> Result<bool> {
        let n = self
            .pool
            .execute(
                "INSERT INTO uploads (id, owner, filename, media_type, data, size, created) \
                 SELECT $1,$2,$3,$4,$5,$6::integer,$7 WHERE (SELECT COALESCE(SUM(size), 0) FROM uploads WHERE owner = $2) + $6::integer <= $8::bigint",
                &[&u.id, &u.owner, &u.filename, &u.media_type, &u.data, &u.size, &u.created, &cap],
            )
            .await?;
        Ok(n > 0)
    }

    /// The uploads of `owner` among `ids`, in the order asked (others are not returned).
    pub async fn uploads_owned(&self, owner: &str, ids: &[String]) -> Result<Vec<UploadRow>> {
        let mut out = Vec::new();
        for id in ids {
            let row = self
                .pool
                .query_opt(
                    "SELECT id, owner, filename, media_type, data, size, created FROM uploads WHERE id = $1 AND owner = $2",
                    &[id, &owner],
                )
                .await?;
            if let Some(row) = row {
                out.push(UploadRow {
                    id: row.get("id")?,
                    owner: row.get("owner")?,
                    filename: row.get("filename")?,
                    media_type: row.get("media_type")?,
                    data: row.get("data")?,
                    size: row.get("size")?,
                    created: row.get("created")?,
                });
            }
        }
        Ok(out)
    }

    /// Bytes a person has waiting in uploads (the quota).
    pub async fn uploads_bytes_of(&self, owner: &str) -> Result<i64> {
        let row = self
            .pool
            .query_one(
                "SELECT COALESCE(SUM(size), 0)::BIGINT AS n FROM uploads WHERE owner = $1",
                &[&owner],
            )
            .await?;
        Ok(row.get("n")?)
    }

    pub async fn uploads_delete(&self, owner: &str, ids: &[String]) -> Result<()> {
        for id in ids {
            self.pool
                .execute(
                    "DELETE FROM uploads WHERE id = $1 AND owner = $2",
                    &[id, &owner],
                )
                .await?;
        }
        Ok(())
    }

    pub async fn upload_delete_one(&self, owner: &str, id: &str) -> Result<bool> {
        Ok(self
            .pool
            .execute(
                "DELETE FROM uploads WHERE id = $1 AND owner = $2",
                &[&id, &owner],
            )
            .await?
            > 0)
    }

    pub async fn uploads_prune(&self, before: i64) -> Result<u64> {
        Ok(self
            .pool
            .execute("DELETE FROM uploads WHERE created < $1", &[&before])
            .await?)
    }
}
