//! Typed links between two issues - "blocks" (its inverse, "blocked by", is
//! the same row read from the other end) and the symmetric "relates to" -
//! distinct from `issue::Issue::parent_id`'s subtask hierarchy.

use crate::domain::db::{WorkbenchError, pool, schema};
use chrono::{DateTime, Utc};
use quench_db::prelude::Db;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IssueLink {
    pub id: String,
    pub issue_id: String,
    pub linked_issue_id: String,
    pub kind: String,
    pub created_at: DateTime<Utc>,
}

pub const KINDS: [&str; 2] = ["blocks", "relates_to"];

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

    let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(&id)
        .bind(&new.issue_id)
        .bind(&new.linked_issue_id)
        .bind(&new.kind)
        .fetch_one(pool)
        .await?;

    from_row(&row)
}

/// Looked up before a delete, to resolve back to the owning issue (and from
/// there the project) an authorization check needs.
pub async fn read(db: &Db, id: &str) -> Result<Option<IssueLink>, WorkbenchError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!("SELECT {COLUMNS} FROM {schema}.issue_links WHERE id = $1");

    let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(id)
        .fetch_optional(pool)
        .await?;

    row.as_ref().map(from_row).transpose()
}

pub async fn delete(db: &Db, id: &str) -> Result<bool, WorkbenchError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!("DELETE FROM {schema}.issue_links WHERE id = $1");

    let result = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(id)
        .execute(pool)
        .await?;

    Ok(result.rows_affected() > 0)
}

/// One side of a resolved link, carrying enough of the linked issue to render
/// it as `{project_key}-{seq}` plus its title, without a caller having to
/// chase the linked issue's project down separately.
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
     FROM {schema}.issue_links il \
     JOIN {schema}.issues i ON i.id = {other} \
     JOIN {schema}.projects p ON p.id = i.project_id \
     WHERE {filter} ORDER BY p.key, i.seq";

/// Every link touching `issue_id`, resolved and split into the three lists a
/// detail page renders: issues this one blocks, issues blocking this one, and
/// issues it loosely relates to (read from either side, since that link has
/// no direction).
pub async fn related(db: &Db, issue_id: &str) -> Result<RelatedIssues, WorkbenchError> {
    let pool = pool(db)?;
    let schema = schema();

    let blocks_sql = LINKED_ISSUE_SELECT
        .replace("{schema}", &schema)
        .replace("{other}", "il.linked_issue_id")
        .replace("{filter}", "il.issue_id = $1 AND il.kind = 'blocks'");
    let blocks = sqlx::query(sqlx::AssertSqlSafe(blocks_sql.as_str()))
        .bind(issue_id)
        .fetch_all(pool)
        .await?;

    let blocked_by_sql = LINKED_ISSUE_SELECT
        .replace("{schema}", &schema)
        .replace("{other}", "il.issue_id")
        .replace("{filter}", "il.linked_issue_id = $1 AND il.kind = 'blocks'");
    let blocked_by = sqlx::query(sqlx::AssertSqlSafe(blocked_by_sql.as_str()))
        .bind(issue_id)
        .fetch_all(pool)
        .await?;

    let relates_forward_sql = LINKED_ISSUE_SELECT
        .replace("{schema}", &schema)
        .replace("{other}", "il.linked_issue_id")
        .replace("{filter}", "il.issue_id = $1 AND il.kind = 'relates_to'");
    let relates_backward_sql = LINKED_ISSUE_SELECT
        .replace("{schema}", &schema)
        .replace("{other}", "il.issue_id")
        .replace(
            "{filter}",
            "il.linked_issue_id = $1 AND il.kind = 'relates_to'",
        );
    let mut relates_to = sqlx::query(sqlx::AssertSqlSafe(relates_forward_sql.as_str()))
        .bind(issue_id)
        .fetch_all(pool)
        .await?;
    relates_to.extend(
        sqlx::query(sqlx::AssertSqlSafe(relates_backward_sql.as_str()))
            .bind(issue_id)
            .fetch_all(pool)
            .await?,
    );

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

fn from_row(row: &sqlx::postgres::PgRow) -> Result<IssueLink, WorkbenchError> {
    Ok(IssueLink {
        id: row.try_get("id")?,
        issue_id: row.try_get("issue_id")?,
        linked_issue_id: row.try_get("linked_issue_id")?,
        kind: row.try_get("kind")?,
        created_at: row.try_get::<DateTime<Utc>, _>("created_at")?,
    })
}

fn linked_issue_from_row(row: &sqlx::postgres::PgRow) -> Result<LinkedIssue, WorkbenchError> {
    Ok(LinkedIssue {
        link_id: row.try_get("link_id")?,
        issue_id: row.try_get("issue_id")?,
        project_key: row.try_get("project_key")?,
        seq: row.try_get("seq")?,
        title: row.try_get("title")?,
        status: row.try_get("status")?,
    })
}
