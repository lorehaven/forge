//! Attachments: a row per message, the bytes stored once.

use super::Store;
use crate::error::Result;

#[derive(Debug, Clone)]
pub struct AttachmentRow {
    pub idx: i32,
    pub filename: String,
    pub media_type: String,
    pub size: i32,
    pub blob_id: String,
}

impl Store {
    pub async fn attachments_of(&self, message_id: &str) -> Result<Vec<AttachmentRow>> {
        let rows = self
            .pool
            .query(
                "SELECT idx, filename, media_type, size, blob_id FROM attachments WHERE message_id = $1 ORDER BY idx",
                &[&message_id],
            )
            .await?;
        rows.iter()
            .map(|r| {
                Ok(AttachmentRow {
                    idx: r.get("idx")?,
                    filename: r.get("filename")?,
                    media_type: r.get("media_type")?,
                    size: r.get("size")?,
                    blob_id: r.get("blob_id")?,
                })
            })
            .collect()
    }

    /// The bytes of attachment `idx` of a message.
    pub async fn attachment_data(
        &self,
        message_id: &str,
        idx: i32,
    ) -> Result<Option<(String, Vec<u8>)>> {
        let row = self
            .pool
            .query_opt(
                "SELECT a.filename, b.data FROM attachments a JOIN blobs b ON b.id = a.blob_id \
                 WHERE a.message_id = $1 AND a.idx = $2",
                &[&message_id, &idx],
            )
            .await?;
        row.map(|r| Ok((r.get("filename")?, r.get("data")?)))
            .transpose()
    }

    /// Removes the bytes nothing points at, if they are older than `before` (a delivery in flight has not pointed yet).
    pub async fn blobs_prune(&self, before: i64) -> Result<u64> {
        Ok(self
            .pool
            .execute(
                "DELETE FROM blobs b WHERE b.created < $1 AND NOT EXISTS (SELECT 1 FROM attachments a WHERE a.blob_id = b.id)",
                &[&before],
            )
            .await?)
    }
}
