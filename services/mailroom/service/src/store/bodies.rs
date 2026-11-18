//! Message bodies, kept once a message has been opened.

use super::Store;
use crate::error::Result;

#[derive(Debug, Clone)]
pub struct BodyRow {
    pub message_id: String,
    pub text: String,
    pub html: Option<String>,
    pub cc_json: String,
    pub reply_to_json: String,
    pub list_unsubscribe: String,
    pub attachments_json: String,
    pub inline_json: String,
    pub fetched: i64,
}

impl Store {
    pub async fn body_get(&self, message_id: &str) -> Result<Option<BodyRow>> {
        let row = self
            .pool
            .query_opt(
                "SELECT message_id, text, html, cc_json, reply_to_json, list_unsubscribe, attachments_json, inline_json, fetched \
                 FROM bodies WHERE message_id = $1",
                &[&message_id],
            )
            .await?;
        row.map(|row| {
            Ok(BodyRow {
                message_id: row.get("message_id")?,
                text: row.get("text")?,
                html: row.get("html")?,
                cc_json: row.get("cc_json")?,
                reply_to_json: row.get("reply_to_json")?,
                list_unsubscribe: row.get("list_unsubscribe")?,
                attachments_json: row.get("attachments_json")?,
                inline_json: row.get("inline_json")?,
                fetched: row.get("fetched")?,
            })
        })
        .transpose()
    }

    pub async fn body_put(&self, b: &BodyRow) -> Result<()> {
        self.pool
            .execute(
                "INSERT INTO bodies (message_id, text, html, cc_json, reply_to_json, list_unsubscribe, attachments_json, inline_json, fetched) \
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) \
                 ON CONFLICT (message_id) DO UPDATE SET text=EXCLUDED.text, html=EXCLUDED.html, cc_json=EXCLUDED.cc_json, \
                 reply_to_json=EXCLUDED.reply_to_json, list_unsubscribe=EXCLUDED.list_unsubscribe, \
                 attachments_json=EXCLUDED.attachments_json, inline_json=EXCLUDED.inline_json, fetched=EXCLUDED.fetched",
                &[
                    &b.message_id, &b.text, &b.html, &b.cc_json, &b.reply_to_json, &b.list_unsubscribe,
                    &b.attachments_json, &b.inline_json, &b.fetched,
                ],
            )
            .await?;
        Ok(())
    }

    /// Drops bodies fetched before `before` that nobody opened lately (the cache's bound).
    pub async fn bodies_prune(&self, before: i64) -> Result<u64> {
        Ok(self
            .pool
            .execute("DELETE FROM bodies WHERE fetched < $1", &[&before])
            .await?)
    }
}
