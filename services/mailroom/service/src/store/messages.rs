//! Cached message headers and flags.

use super::Store;
use crate::error::Result;
use std::fmt::Write as _;

#[derive(Debug, Clone)]
pub struct MessageRow {
    pub id: String,
    pub account_id: String,
    pub folder_id: String,
    pub uid: i64,
    pub subject: String,
    pub from_name: String,
    pub from_address: String,
    pub to_json: String,
    pub date: i64,
    pub preview: String,
    pub message_id: String,
    pub in_reply_to: String,
    pub seen: bool,
    pub flagged: bool,
    pub answered: bool,
    pub attachments: bool,
    pub size: i32,
    pub search: String,
}

const COLUMNS: &str = "id, account_id, folder_id, uid, subject, from_name, from_address, to_json, date, preview, \
                       message_id, in_reply_to, seen, flagged, answered, attachments, size, search";
/// The same, qualified for joins.
const MCOLUMNS: &str = "m.id, m.account_id, m.folder_id, m.uid, m.subject, m.from_name, m.from_address, m.to_json, m.date, \
                        m.preview, m.message_id, m.in_reply_to, m.seen, m.flagged, m.answered, m.attachments, m.size, m.search";

fn from_row(row: &quench_pg::Row) -> Result<MessageRow> {
    Ok(MessageRow {
        id: row.get("id")?,
        account_id: row.get("account_id")?,
        folder_id: row.get("folder_id")?,
        uid: row.get("uid")?,
        subject: row.get("subject")?,
        from_name: row.get("from_name")?,
        from_address: row.get("from_address")?,
        to_json: row.get("to_json")?,
        date: row.get("date")?,
        preview: row.get("preview")?,
        message_id: row.get("message_id")?,
        in_reply_to: row.get("in_reply_to")?,
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

impl Store {
    /// Adds a message (a second sighting of the same UID changes nothing).
    pub async fn message_insert(&self, m: &MessageRow) -> Result<bool> {
        let n = self
            .pool
            .execute(
                "INSERT INTO messages (id, account_id, folder_id, uid, subject, from_name, from_address, to_json, date, \
                 preview, message_id, in_reply_to, seen, flagged, answered, attachments, size, search) \
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18) \
                 ON CONFLICT (folder_id, uid) DO NOTHING",
                &[
                    &m.id, &m.account_id, &m.folder_id, &m.uid, &m.subject, &m.from_name, &m.from_address, &m.to_json,
                    &m.date, &m.preview, &m.message_id, &m.in_reply_to, &m.seen, &m.flagged, &m.answered,
                    &m.attachments, &m.size, &m.search,
                ],
            )
            .await?;
        Ok(n > 0)
    }

    pub async fn message_get(&self, id: &str) -> Result<Option<MessageRow>> {
        let row = self
            .pool
            .query_opt(
                &format!("SELECT {COLUMNS} FROM messages WHERE id = $1"),
                &[&id],
            )
            .await?;
        row.as_ref().map(from_row).transpose()
    }

    /// A message, if it is in a mailbox `owner` owns.
    pub async fn message_get_owned(&self, id: &str, owner: &str) -> Result<Option<MessageRow>> {
        let row = self
            .pool
            .query_opt(
                &format!(
                    "SELECT {MCOLUMNS} FROM messages m JOIN accounts a ON a.id = m.account_id WHERE m.id = $1 AND a.owner = $2"
                ),
                &[&id, &owner],
            )
            .await?;
        row.as_ref().map(from_row).transpose()
    }

    /// The messages of a folder, newest first.
    pub async fn messages_in_folder(
        &self,
        folder_id: &str,
        query: &Query,
        offset: i64,
        limit: i64,
    ) -> Result<(Vec<MessageRow>, i64)> {
        let (where_clause, params) = filter("m.folder_id = $1", vec![folder_id.to_string()], query);
        self.page(&where_clause, &params, "", offset, limit).await
    }

    /// The messages of every folder with `role` across the mailboxes of `owner`, newest first (the unified inbox).
    pub async fn messages_by_role(
        &self,
        owner: &str,
        role: &str,
        query: &Query,
        offset: i64,
        limit: i64,
    ) -> Result<(Vec<MessageRow>, i64)> {
        let (where_clause, params) = filter(
            "a.owner = $1 AND f.role = $2 AND NOT a.disabled",
            vec![owner.to_string(), role.to_string()],
            query,
        );
        self.page(
            &where_clause,
            &params,
            "JOIN accounts a ON a.id = m.account_id JOIN folders f ON f.id = m.folder_id",
            offset,
            limit,
        )
        .await
    }

    async fn page(
        &self,
        where_clause: &str,
        params: &[String],
        joins: &str,
        offset: i64,
        limit: i64,
    ) -> Result<(Vec<MessageRow>, i64)> {
        let refs: Vec<&dyn quench_pg::ToSql> =
            params.iter().map(|p| p as &dyn quench_pg::ToSql).collect();
        let joins = if joins.is_empty() { "" } else { joins };
        let count = self
            .pool
            .query_one(
                &format!("SELECT COUNT(*) AS n FROM messages m {joins} WHERE {where_clause}"),
                &refs,
            )
            .await?;
        let total: i64 = count.get("n")?;
        let mut all: Vec<&dyn quench_pg::ToSql> = refs;
        all.push(&offset);
        all.push(&limit);
        let (o, l) = (all.len() - 1, all.len());
        let rows = self
            .pool
            .query(
                &format!(
                    "SELECT {MCOLUMNS} FROM messages m {joins} WHERE {where_clause} \
                     ORDER BY m.date DESC, m.uid DESC OFFSET ${o} LIMIT ${l}"
                ),
                &all,
            )
            .await?;
        Ok((rows.iter().map(from_row).collect::<Result<_>>()?, total))
    }

    /// The UIDs cached for a folder.
    pub async fn message_uids(&self, folder_id: &str) -> Result<Vec<i64>> {
        let rows = self
            .pool
            .query(
                "SELECT uid FROM messages WHERE folder_id = $1 ORDER BY uid",
                &[&folder_id],
            )
            .await?;
        rows.iter().map(|r| Ok(r.get("uid")?)).collect()
    }

    /// Forgets messages the server no longer has.
    pub async fn messages_delete_uids(&self, folder_id: &str, uids: &[i64]) -> Result<()> {
        if uids.is_empty() {
            return Ok(());
        }
        let list = quench_pg::PgArray(uids.to_vec());
        self.pool
            .execute(
                "DELETE FROM messages WHERE folder_id = $1 AND uid = ANY($2::bigint[])",
                &[&folder_id, &list],
            )
            .await?;
        Ok(())
    }

    /// Forgets everything cached in a folder (its numbering changed).
    pub async fn messages_clear_folder(&self, folder_id: &str) -> Result<()> {
        self.pool
            .execute("DELETE FROM messages WHERE folder_id = $1", &[&folder_id])
            .await?;
        Ok(())
    }

    /// Forgets messages by id.
    pub async fn messages_delete_ids(&self, ids: &[String]) -> Result<()> {
        if ids.is_empty() {
            return Ok(());
        }
        let list = quench_pg::PgArray(ids.to_vec());
        self.pool
            .execute("DELETE FROM messages WHERE id = ANY($1::text[])", &[&list])
            .await?;
        Ok(())
    }

    /// Sets the flags the server reports for a UID.
    pub async fn message_set_flags(
        &self,
        folder_id: &str,
        uid: i64,
        seen: bool,
        flagged: bool,
        answered: bool,
    ) -> Result<()> {
        self.pool
            .execute(
                "UPDATE messages SET seen=$3, flagged=$4, answered=$5 WHERE folder_id=$1 AND uid=$2 \
                 AND (seen <> $3 OR flagged <> $4 OR answered <> $5)",
                &[&folder_id, &uid, &seen, &flagged, &answered],
            )
            .await?;
        Ok(())
    }

    /// After a change the person made: the flags as the server now has them.
    pub async fn message_set_state(
        &self,
        id: &str,
        seen: Option<bool>,
        flagged: Option<bool>,
        answered: Option<bool>,
    ) -> Result<()> {
        self.pool
            .execute(
                "UPDATE messages SET seen = COALESCE($2, seen), flagged = COALESCE($3, flagged), answered = COALESCE($4, answered) WHERE id = $1",
                &[&id, &seen, &flagged, &answered],
            )
            .await?;
        Ok(())
    }

    /// The preview text, once a body is known.
    pub async fn message_set_preview(&self, id: &str, preview: &str) -> Result<()> {
        self.pool
            .execute(
                "UPDATE messages SET preview = $2 WHERE id = $1 AND preview = ''",
                &[&id, &preview],
            )
            .await?;
        Ok(())
    }

    /// The newest messages of a folder that have no body cached and are small enough to prefetch.
    pub async fn messages_to_prefetch(
        &self,
        folder_id: &str,
        max_size: i32,
        limit: i64,
    ) -> Result<Vec<MessageRow>> {
        let rows = self
            .pool
            .query(
                &format!(
                    "SELECT {MCOLUMNS} FROM messages m LEFT JOIN bodies b ON b.message_id = m.id \
                     WHERE m.folder_id = $1 AND b.message_id IS NULL AND m.size <= $2 ORDER BY m.date DESC, m.uid DESC LIMIT $3"
                ),
                &[&folder_id, &max_size, &limit],
            )
            .await?;
        rows.iter().map(from_row).collect()
    }

    /// Where a message was answered from: the message with this Message-ID in a person's mailboxes.
    pub async fn message_by_message_id(
        &self,
        owner: &str,
        message_id: &str,
    ) -> Result<Option<MessageRow>> {
        let row = self
            .pool
            .query_opt(
                &format!(
                    "SELECT {MCOLUMNS} FROM messages m JOIN accounts a ON a.id = m.account_id \
                     WHERE a.owner = $1 AND m.message_id = $2 ORDER BY m.date DESC LIMIT 1"
                ),
                &[&owner, &message_id],
            )
            .await?;
        row.as_ref().map(from_row).transpose()
    }

    /// How many messages are cached in all.
    pub async fn messages_count(&self) -> Result<i64> {
        let row = self
            .pool
            .query_one("SELECT COUNT(*) AS n FROM messages", &[])
            .await?;
        Ok(row.get("n")?)
    }
}

/// The WHERE clause and its parameters for a query, after the clause the caller starts with.
fn filter(base: &str, mut params: Vec<String>, query: &Query) -> (String, Vec<String>) {
    let mut clause = base.to_string();
    if query.unread_only {
        clause.push_str(" AND NOT m.seen");
    }
    if query.flagged_only {
        clause.push_str(" AND m.flagged");
    }
    for term in &query.terms {
        params.push(like(term));
        let _ = write!(clause, " AND m.search LIKE ${} ESCAPE '\\'", params.len());
    }
    (clause, params)
}
