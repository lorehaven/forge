//! Typed links between issues ("`blocks"/"relates_to`"), distinct from
//! `Issue::parent_id`'s subtask hierarchy.

use crate::domain::db::{WorkbenchError, pool, schema};
use quench_core::time::Time;
use quench_core::uuid::Uuid;
use quench_db::prelude::Db;
use quench_serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IssueLink {
    pub id: String,
    pub issue_id: String,
    pub linked_issue_id: String,
    pub kind: String,
    pub created_at: Time,
}

pub const KINDS: [&str; 2] = ["blocks", "relates_to"];

#[must_use]
pub fn is_valid_kind(kind: &str) -> bool {
    KINDS.contains(&kind)
}

#[derive(Clone, Debug)]
pub struct NewIssueLink {
    pub issue_id: String,
    pub linked_issue_id: String,
    pub kind: String,
}

pub async fn create(db: &Db, new: &NewIssueLink) -> Result<IssueLink, WorkbenchError> {
    let pool = pool(db)?;
    let schema = schema();
    let id = Uuid::new_v4().to_string();

    let sql = format!(
        "INSERT INTO {schema}.issue_links (id, issue_id, linked_issue_id, kind) \
         VALUES ($1, $2, $3, $4) RETURNING {COLUMNS}"
    );

    let row = pool
        .query_one(&sql, &[&id, &new.issue_id, &new.linked_issue_id, &new.kind])
        .await?;

    from_row(&row)
}

/// Used before delete to resolve the owning issue/project for auth.
pub async fn read(db: &Db, id: &str) -> Result<Option<IssueLink>, WorkbenchError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!("SELECT {COLUMNS} FROM {schema}.issue_links WHERE id = $1");

    let row = pool.query_opt(&sql, &[&id]).await?;

    row.as_ref().map(from_row).transpose()
}

pub async fn delete(db: &Db, id: &str) -> Result<bool, WorkbenchError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!("DELETE FROM {schema}.issue_links WHERE id = $1");

    let result = pool.execute(&sql, &[&id]).await?;

    Ok(result > 0)
}

/// One side of a resolved link: enough of the linked issue to render it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LinkedIssue {
    pub link_id: String,
    pub issue_id: String,
    pub project_key: String,
    pub seq: i32,
    pub title: String,
    pub status: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RelatedIssues {
    /// Issues that cannot proceed until this one is done.
    pub blocks: Vec<LinkedIssue>,
    /// Issues this one cannot proceed until they're done.
    pub blocked_by: Vec<LinkedIssue>,
    /// Loosely related issues, no direction implied.
    pub relates_to: Vec<LinkedIssue>,
}

const LINKED_ISSUE_SELECT: &str = "SELECT il.id AS link_id, i.id AS issue_id, \
     p.key AS project_key, i.seq AS seq, i.title AS title, i.status AS status \
     FROM __schema__.issue_links il \
     JOIN __schema__.issues i ON i.id = __other__ \
     JOIN __schema__.projects p ON p.id = i.project_id \
     WHERE __filter__ ORDER BY p.key, i.seq";

/// Links touching `issue_id`, split into `blocks/blocked_by/relates_to`.
pub async fn related(db: &Db, issue_id: &str) -> Result<RelatedIssues, WorkbenchError> {
    let pool = pool(db)?;
    let schema = schema();

    let blocks_sql = LINKED_ISSUE_SELECT
        .replace("__schema__", &schema)
        .replace("__other__", "il.linked_issue_id")
        .replace("__filter__", "il.issue_id = $1 AND il.kind = 'blocks'");
    let blocks = pool.query(&blocks_sql, &[&issue_id]).await?;

    let blocked_by_sql = LINKED_ISSUE_SELECT
        .replace("__schema__", &schema)
        .replace("__other__", "il.issue_id")
        .replace(
            "__filter__",
            "il.linked_issue_id = $1 AND il.kind = 'blocks'",
        );
    let blocked_by = pool.query(&blocked_by_sql, &[&issue_id]).await?;

    let relates_forward_sql = LINKED_ISSUE_SELECT
        .replace("__schema__", &schema)
        .replace("__other__", "il.linked_issue_id")
        .replace("__filter__", "il.issue_id = $1 AND il.kind = 'relates_to'");
    let relates_backward_sql = LINKED_ISSUE_SELECT
        .replace("__schema__", &schema)
        .replace("__other__", "il.issue_id")
        .replace(
            "__filter__",
            "il.linked_issue_id = $1 AND il.kind = 'relates_to'",
        );
    let mut relates_to = pool.query(&relates_forward_sql, &[&issue_id]).await?;
    relates_to.extend(pool.query(&relates_backward_sql, &[&issue_id]).await?);

    Ok(RelatedIssues {
        blocks: blocks
            .iter()
            .map(linked_issue_from_row)
            .collect::<Result<_, _>>()?,
        blocked_by: blocked_by
            .iter()
            .map(linked_issue_from_row)
            .collect::<Result<_, _>>()?,
        relates_to: relates_to
            .iter()
            .map(linked_issue_from_row)
            .collect::<Result<_, _>>()?,
    })
}

const COLUMNS: &str = "id, issue_id, linked_issue_id, kind, created_at";

fn from_row(row: &quench_pg::Row) -> Result<IssueLink, WorkbenchError> {
    Ok(IssueLink {
        id: row.get("id")?,
        issue_id: row.get("issue_id")?,
        linked_issue_id: row.get("linked_issue_id")?,
        kind: row.get("kind")?,
        created_at: row.get::<Time, _>("created_at")?,
    })
}

fn linked_issue_from_row(row: &quench_pg::Row) -> Result<LinkedIssue, WorkbenchError> {
    Ok(LinkedIssue {
        link_id: row.get("link_id")?,
        issue_id: row.get("issue_id")?,
        project_key: row.get("project_key")?,
        seq: row.get("seq")?,
        title: row.get("title")?,
        status: row.get("status")?,
    })
}
