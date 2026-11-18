//! Messages: the rows lists read, and the bodies read one at a time.

use super::Store;
use crate::error::Result;
use std::fmt::Write as _;

#[derive(Debug, Clone)]
pub struct MessageRow {
    pub id: String,
    pub owner: String,
    pub folder: String,
    pub message_id: String,
    pub in_reply_to: String,
    pub from_name: String,
    pub from_address: String,
    pub source: String,
    pub to_json: String,
    pub cc_json: String,
    pub subject: String,
    pub unsubscribe: String,
    pub date: i64,
    pub moved: i64,
    pub preview: String,
    pub seen: bool,
    pub flagged: bool,
    pub answered: bool,
    pub attachments: bool,
    pub size: i32,
    pub search: String,
}

#[derive(Debug, Clone, Default)]
pub struct BodyRow {
    pub text: String,
    pub html: Option<String>,
}

const COLUMNS: &str = "id, owner, folder, message_id, in_reply_to, from_name, from_address, source, to_json, cc_json, \
                       subject, unsubscribe, date, moved, preview, seen, flagged, answered, attachments, size, search";

fn from_row(row: &quench_pg::Row) -> Result<MessageRow> {
    Ok(MessageRow {
        id: row.get("id")?,
        owner: row.get("owner")?,
        folder: row.get("folder")?,
        message_id: row.get("message_id")?,
        in_reply_to: row.get("in_reply_to")?,
        from_name: row.get("from_name")?,
        from_address: row.get("from_address")?,
        source: row.get("source")?,
        to_json: row.get("to_json")?,
        cc_json: row.get("cc_json")?,
        subject: row.get("subject")?,
        unsubscribe: row.get("unsubscribe")?,
        date: row.get("date")?,
        moved: row.get("moved")?,
        preview: row.get("preview")?,
        seen: row.get("seen")?,
        flagged: row.get("flagged")?,
        answered: row.get("answered")?,
        attachments: row.get("attachments")?,
        size: row.get("size")?,
        search: row.get("search")?,
    })
}

/// What a list asks for.
#[derive(Debug, Clone, Default)]
pub struct Query {
    pub unread_only: bool,
    pub flagged_only: bool,
    /// Lower-cased words; every one must be found.
    pub terms: Vec<String>,
}

/// `%word%` with LIKE's own characters escaped.
fn like(term: &str) -> String {
    let mut out = String::from("%");
    for c in term.chars() {
        if matches!(c, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('%');
    out
}

/// What goes into one mailbox.
#[derive(Debug, Clone)]
pub struct Copy<'a> {
    pub row: &'a MessageRow,
    pub body: &'a BodyRow,
    /// `(filename, media type, size, blob id)` in order.
    pub attachments: &'a [(String, String, i32, String)],
}

impl Store {
    pub async fn message_get_owned(&self, id: &str, owner: &str) -> Result<Option<MessageRow>> {
        let row = self
            .pool
            .query_opt(
                &format!("SELECT {COLUMNS} FROM messages WHERE id = $1 AND owner = $2"),
                &[&id, &owner],
            )
            .await?;
        row.as_ref().map(from_row).transpose()
    }

    pub async fn message_body(&self, id: &str) -> Result<BodyRow> {
        let row = self
            .pool
            .query_opt(
                "SELECT text, html FROM bodies WHERE message_id = $1",
                &[&id],
            )
            .await?;
        row.map_or_else(
            || Ok(BodyRow::default()),
            |r| {
                Ok(BodyRow {
                    text: r.get("text")?,
                    html: r.get("html")?,
                })
            },
        )
    }

    /// The messages of a folder, newest first.
    pub async fn messages_page(
        &self,
        owner: &str,
        folder: &str,
        query: &Query,
        offset: i64,
        limit: i64,
    ) -> Result<(Vec<MessageRow>, i64)> {
        let mut clause = String::from("owner = $1 AND folder = $2");
        let mut params: Vec<String> = vec![owner.to_string(), folder.to_string()];
        if query.unread_only {
            clause.push_str(" AND NOT seen");
        }
        if query.flagged_only {
            clause.push_str(" AND flagged");
        }
        for term in &query.terms {
            params.push(like(term));
            let _ = write!(clause, " AND search LIKE ${} ESCAPE '\\'", params.len());
        }
        let refs: Vec<&dyn quench_pg::ToSql> =
            params.iter().map(|p| p as &dyn quench_pg::ToSql).collect();
        let count = self
            .pool
            .query_one(
                &format!("SELECT COUNT(*) AS n FROM messages WHERE {clause}"),
                &refs,
            )
            .await?;
        let total: i64 = count.get("n")?;
        let mut all = refs;
        all.push(&offset);
        all.push(&limit);
        let (o, l) = (all.len() - 1, all.len());
        let rows = self
            .pool
            .query(
                &format!(
                    "SELECT {COLUMNS} FROM messages WHERE {clause} ORDER BY date DESC, seq DESC OFFSET ${o} LIMIT ${l}"
                ),
                &all,
            )
            .await?;
        Ok((rows.iter().map(from_row).collect::<Result<_>>()?, total))
    }

    /// `(folder, unread, total)` for the folders that hold anything.
    pub async fn folder_counts(&self, owner: &str) -> Result<Vec<(String, i64, i64)>> {
        let rows = self
            .pool
            .query(
                "SELECT folder, COUNT(*) FILTER (WHERE NOT seen)::BIGINT AS unread, COUNT(*)::BIGINT AS total \
                 FROM messages WHERE owner = $1 GROUP BY folder",
                &[&owner],
            )
            .await?;
        rows.iter()
            .map(|r| Ok((r.get("folder")?, r.get("unread")?, r.get("total")?)))
            .collect()
    }

    /// Bytes a mailbox holds.
    pub async fn mailbox_bytes(&self, owner: &str) -> Result<i64> {
        let row = self
            .pool
            .query_one(
                "SELECT COALESCE(SUM(size), 0)::BIGINT AS n FROM messages WHERE owner = $1",
                &[&owner],
            )
            .await?;
        Ok(row.get("n")?)
    }

    pub async fn message_set_flags(
        &self,
        id: &str,
        owner: &str,
        seen: Option<bool>,
        flagged: Option<bool>,
    ) -> Result<()> {
        if let Some(seen) = seen {
            self.pool
                .execute(
                    "UPDATE messages SET seen = $3 WHERE id = $1 AND owner = $2",
                    &[&id, &owner, &seen],
                )
                .await?;
        }
        if let Some(flagged) = flagged {
            self.pool
                .execute(
                    "UPDATE messages SET flagged = $3 WHERE id = $1 AND owner = $2",
                    &[&id, &owner, &flagged],
                )
                .await?;
        }
        Ok(())
    }

    pub async fn message_mark_answered(&self, id: &str, owner: &str) -> Result<()> {
        self.pool
            .execute(
                "UPDATE messages SET answered = TRUE WHERE id = $1 AND owner = $2",
                &[&id, &owner],
            )
            .await?;
        Ok(())
    }

    pub async fn message_move(&self, id: &str, owner: &str, folder: &str, now: i64) -> Result<()> {
        self.pool
            .execute(
                "UPDATE messages SET folder = $3, moved = $4 WHERE id = $1 AND owner = $2",
                &[&id, &owner, &folder, &now],
            )
            .await?;
        Ok(())
    }

    pub async fn message_delete(&self, id: &str, owner: &str) -> Result<()> {
        self.pool
            .execute(
                "DELETE FROM messages WHERE id = $1 AND owner = $2",
                &[&id, &owner],
            )
            .await?;
        Ok(())
    }

    /// Empties what has been in the trash since before `before`.
    pub async fn trash_prune(&self, before: i64) -> Result<u64> {
        Ok(self
            .pool
            .execute(
                "DELETE FROM messages WHERE folder = 'trash' AND moved < $1",
                &[&before],
            )
            .await?)
    }

    /// `(messages, bytes)` over every mailbox.
    pub async fn message_totals(&self) -> Result<(i64, i64)> {
        let row = self
            .pool
            .query_one(
                "SELECT COUNT(*)::BIGINT AS n, COALESCE(SUM(size), 0)::BIGINT AS bytes FROM messages",
                &[],
            )
            .await?;
        Ok((row.get("n")?, row.get("bytes")?))
    }

    /// Writes one message into several mailboxes, with its attachments, all or nothing. The blobs go in first (once each).
    pub async fn deliver_all(
        &self,
        copies: &[Copy<'_>],
        blobs: &[(String, Vec<u8>)],
        now: i64,
    ) -> Result<()> {
        let mut conn = self.pool.get().await?;
        let mut tx = conn.begin().await?;
        for (id, data) in blobs {
            tx.execute(
                "INSERT INTO blobs (id, data, created) VALUES ($1, $2, $3)",
                &[id, data, &now],
            )
            .await?;
        }
        for copy in copies {
            let m = copy.row;
            tx.execute(
                "INSERT INTO messages (id, owner, folder, message_id, in_reply_to, from_name, from_address, source, to_json, \
                 cc_json, subject, unsubscribe, date, moved, preview, seen, flagged, answered, attachments, size, search) \
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21)",
                &[
                    &m.id, &m.owner, &m.folder, &m.message_id, &m.in_reply_to, &m.from_name, &m.from_address, &m.source,
                    &m.to_json, &m.cc_json, &m.subject, &m.unsubscribe, &m.date, &m.moved, &m.preview, &m.seen,
                    &m.flagged, &m.answered, &m.attachments, &m.size, &m.search,
                ],
            )
            .await?;
            tx.execute(
                "INSERT INTO bodies (message_id, text, html) VALUES ($1, $2, $3)",
                &[&m.id, &copy.body.text, &copy.body.html],
            )
            .await?;
            for (idx, (filename, media_type, size, blob_id)) in copy.attachments.iter().enumerate()
            {
                let idx = i32::try_from(idx).unwrap_or(i32::MAX);
                tx.execute(
                    "INSERT INTO attachments (message_id, idx, filename, media_type, size, blob_id) VALUES ($1,$2,$3,$4,$5,$6)",
                    &[&m.id, &idx, filename, media_type, size, blob_id],
                )
                .await?;
            }
        }
        tx.commit().await?;
        Ok(())
    }
}
