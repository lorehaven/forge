//! Reading and writing conveyor's organisational tree.
//!
//! Raw SQL, for the same reason `repos.rs` is: the tree walks below need a
//! recursive query `quench-db`'s `Crud` has no way to express, and having half
//! the schema go through one path and half through another is worse than
//! having all of it go through this one.

use crate::domain::Project;
use crate::scheduler::queue::{QueueError, pool, schema};
use chrono::{DateTime, Utc};
use quench_db::prelude::Db;
use sqlx::Row;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct NewProject {
    pub name: String,
    /// `None` registers a root node.
    pub parent_id: Option<String>,
}

pub async fn create(db: &Db, new: &NewProject) -> Result<Project, QueueError> {
    let pool = pool(db)?;
    let schema = schema();
    let id = Uuid::new_v4().to_string();

    let sql = format!(
        "INSERT INTO {schema}.projects (id, name, parent_id) \
         VALUES ($1, $2, $3) RETURNING {COLUMNS}"
    );

    let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(&id)
        .bind(&new.name)
        .bind(&new.parent_id)
        .fetch_one(pool)
        .await?;

    from_row(&row)
}

pub async fn read(db: &Db, id: &str) -> Result<Option<Project>, QueueError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!("SELECT {COLUMNS} FROM {schema}.projects WHERE id = $1");

    let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(id)
        .fetch_optional(pool)
        .await?;

    row.as_ref().map(from_row).transpose()
}

/// The direct children of a node, or the roots when `parent_id` is `None`.
pub async fn list_children(db: &Db, parent_id: Option<&str>) -> Result<Vec<Project>, QueueError> {
    let pool = pool(db)?;
    let schema = schema();

    let rows = match parent_id {
        Some(parent_id) => {
            let sql = format!(
                "SELECT {COLUMNS} FROM {schema}.projects WHERE parent_id = $1 ORDER BY name"
            );
            sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
                .bind(parent_id)
                .fetch_all(pool)
                .await?
        }
        None => {
            let sql = format!(
                "SELECT {COLUMNS} FROM {schema}.projects WHERE parent_id IS NULL ORDER BY name"
            );
            sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
                .fetch_all(pool)
                .await?
        }
    };

    rows.iter().map(from_row).collect()
}

/// Every node in the tree, in one query - for a caller building the whole
/// hierarchy in memory (the UI's project tree) rather than walking it one
/// level at a time.
pub async fn list_all(db: &Db) -> Result<Vec<Project>, QueueError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!("SELECT {COLUMNS} FROM {schema}.projects ORDER BY name");

    let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .fetch_all(pool)
        .await?;

    rows.iter().map(from_row).collect()
}

pub async fn rename(db: &Db, id: &str, name: &str) -> Result<Option<Project>, QueueError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!(
        "UPDATE {schema}.projects SET name = $2, updated_at = NOW() \
         WHERE id = $1 RETURNING {COLUMNS}"
    );

    let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(id)
        .bind(name)
        .fetch_optional(pool)
        .await?;

    row.as_ref().map(from_row).transpose()
}

pub enum MoveOutcome {
    Moved(Project),
    NotFound,
    /// The requested parent is the node itself or one of its own descendants.
    WouldCycle,
}

pub async fn move_to(
    db: &Db,
    id: &str,
    parent_id: Option<&str>,
) -> Result<MoveOutcome, QueueError> {
    if let Some(parent_id) = parent_id {
        if parent_id == id {
            return Ok(MoveOutcome::WouldCycle);
        }
        let descendants = descendant_ids(db, std::slice::from_ref(&id.to_string())).await?;
        if descendants.iter().any(|descendant| descendant == parent_id) {
            return Ok(MoveOutcome::WouldCycle);
        }
    }

    let pool = pool(db)?;
    let schema = schema();
    let sql = format!(
        "UPDATE {schema}.projects SET parent_id = $2, updated_at = NOW() \
         WHERE id = $1 RETURNING {COLUMNS}"
    );

    let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(id)
        .bind(parent_id)
        .fetch_optional(pool)
        .await?;

    match row.as_ref().map(from_row).transpose()? {
        Some(project) => Ok(MoveOutcome::Moved(project)),
        None => Ok(MoveOutcome::NotFound),
    }
}

pub enum DeleteOutcome {
    Deleted,
    NotFound,
    HasChildren,
    /// A repository is still attached; detach or delete it first.
    HasRepo,
}

/// Refuses a delete that would silently orphan children or a repository,
/// rather than relying on `ON DELETE CASCADE` to make the call for it - this
/// is meant to be explicit and deliberate, the same reason repo registration
/// is.
pub async fn delete(db: &Db, id: &str) -> Result<DeleteOutcome, QueueError> {
    let pool = pool(db)?;
    let schema = schema();

    let exists_sql = format!("SELECT 1 FROM {schema}.projects WHERE id = $1");
    let exists = sqlx::query(sqlx::AssertSqlSafe(exists_sql.as_str()))
        .bind(id)
        .fetch_optional(pool)
        .await?
        .is_some();
    if !exists {
        return Ok(DeleteOutcome::NotFound);
    }

    let has_children_sql = format!("SELECT 1 FROM {schema}.projects WHERE parent_id = $1 LIMIT 1");
    let has_children = sqlx::query(sqlx::AssertSqlSafe(has_children_sql.as_str()))
        .bind(id)
        .fetch_optional(pool)
        .await?
        .is_some();
    if has_children {
        return Ok(DeleteOutcome::HasChildren);
    }

    let has_repo_sql = format!("SELECT 1 FROM {schema}.repos WHERE project_id = $1 LIMIT 1");
    let has_repo = sqlx::query(sqlx::AssertSqlSafe(has_repo_sql.as_str()))
        .bind(id)
        .fetch_optional(pool)
        .await?
        .is_some();
    if has_repo {
        return Ok(DeleteOutcome::HasRepo);
    }

    let delete_sql = format!("DELETE FROM {schema}.projects WHERE id = $1");
    sqlx::query(sqlx::AssertSqlSafe(delete_sql.as_str()))
        .bind(id)
        .execute(pool)
        .await?;

    Ok(DeleteOutcome::Deleted)
}

/// `id` and every ancestor above it, in no particular order - callers only
/// ever check membership. Used by permission enforcement: a grant on any
/// entry in this chain covers `id`.
pub async fn ancestor_chain(db: &Db, id: &str) -> Result<Vec<String>, QueueError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!(
        "WITH RECURSIVE chain AS ( \
             SELECT id, parent_id FROM {schema}.projects WHERE id = $1 \
             UNION ALL \
             SELECT p.id, p.parent_id FROM {schema}.projects p JOIN chain c ON p.id = c.parent_id \
         ) SELECT id FROM chain"
    );

    let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(id)
        .fetch_all(pool)
        .await?;

    rows.iter()
        .map(|row| row.try_get::<String, _>("id").map_err(QueueError::from))
        .collect()
}

/// Every one of `root_ids` plus everything nested beneath them. Used to turn a
/// set of directly-granted project ids into the full set of projects a grant
/// on them covers - the read side of the same rule `ancestor_chain` checks
/// from the other direction.
pub async fn descendant_ids(db: &Db, root_ids: &[String]) -> Result<Vec<String>, QueueError> {
    if root_ids.is_empty() {
        return Ok(Vec::new());
    }

    let pool = pool(db)?;
    let schema = schema();
    let sql = format!(
        "WITH RECURSIVE tree AS ( \
             SELECT id FROM {schema}.projects WHERE id = ANY($1) \
             UNION ALL \
             SELECT p.id FROM {schema}.projects p JOIN tree t ON p.parent_id = t.id \
         ) SELECT id FROM tree"
    );

    let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(root_ids)
        .fetch_all(pool)
        .await?;

    rows.iter()
        .map(|row| row.try_get::<String, _>("id").map_err(QueueError::from))
        .collect()
}

/// `root/.../leaf`, the organisational equivalent of `Repo::slug()`. `None`
/// when `id` does not exist.
pub async fn full_path(db: &Db, id: &str) -> Result<Option<String>, QueueError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!(
        "WITH RECURSIVE chain AS ( \
             SELECT id, name, parent_id, 0 AS depth FROM {schema}.projects WHERE id = $1 \
             UNION ALL \
             SELECT p.id, p.name, p.parent_id, c.depth + 1 \
             FROM {schema}.projects p JOIN chain c ON p.id = c.parent_id \
         ) SELECT name FROM chain ORDER BY depth DESC"
    );

    let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(id)
        .fetch_all(pool)
        .await?;

    if rows.is_empty() {
        return Ok(None);
    }

    let names = rows
        .iter()
        .map(|row| row.try_get::<String, _>("name"))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Some(names.join("/")))
}

const COLUMNS: &str = "id, name, parent_id, created_at, updated_at";

fn from_row(row: &sqlx::postgres::PgRow) -> Result<Project, QueueError> {
    Ok(Project {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        parent_id: row.try_get("parent_id")?,
        created_at: row.try_get::<DateTime<Utc>, _>("created_at")?,
        updated_at: row.try_get::<DateTime<Utc>, _>("updated_at")?,
    })
}
