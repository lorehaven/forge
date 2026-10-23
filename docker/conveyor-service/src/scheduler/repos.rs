//! Reading and writing registered repositories.
//! Raw SQL rather than `Crud`, matching the run queue: half the schema on one path and half on another is worse.

use crate::domain::{Provider, Repo};
use crate::scheduler::queue::{QueueError, pool, schema};
use quench_core::time::Time;
use quench_core::uuid::Uuid;
use quench_db::prelude::Db;

/// What a caller supplies to register a repository. The id, timestamps and
/// enabled flag are conveyor's to decide.
#[derive(Clone, Debug)]
pub struct NewRepo {
    pub provider: Provider,
    pub owner: String,
    pub name: String,
    pub clone_url: String,
    pub default_branch: String,
    pub registered_by: String,
    /// The project node this repo attaches to.
    pub project_id: String,
}

pub async fn create(db: &Db, new: &NewRepo) -> Result<Repo, QueueError> {
    let pool = pool(db)?;
    let schema = schema();
    let id = Uuid::new_v4().to_string();

    let sql = format!(
        "INSERT INTO {schema}.repos \
         (id, provider, owner, name, clone_url, default_branch, registered_by, project_id) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) \
         RETURNING {COLUMNS}"
    );

    let row = pool
        .query_one(
            &sql,
            &[
                &id as &dyn quench_pg::ToSql,
                &new.provider.as_str(),
                &new.owner,
                &new.name,
                &new.clone_url,
                &new.default_branch,
                &new.registered_by,
                &new.project_id,
            ],
        )
        .await?;

    from_row(&row)
}

pub async fn read(db: &Db, id: &str) -> Result<Option<Repo>, QueueError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!("SELECT {COLUMNS} FROM {schema}.repos WHERE id = $1");

    let row = pool
        .query_opt(&sql, &[&id as &dyn quench_pg::ToSql])
        .await?;

    row.as_ref().map(from_row).transpose()
}

/// Looks a repository up the way a webhook identifies one.
pub async fn find_by_slug(
    db: &Db,
    provider: Provider,
    owner: &str,
    name: &str,
) -> Result<Option<Repo>, QueueError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!(
        "SELECT {COLUMNS} FROM {schema}.repos \
         WHERE provider = $1 AND owner = $2 AND name = $3"
    );

    let row = pool
        .query_opt(
            &sql,
            &[&provider.as_str() as &dyn quench_pg::ToSql, &owner, &name],
        )
        .await?;

    row.as_ref().map(from_row).transpose()
}

/// By `owner/name` alone, no provider - unlike `find_by_slug`, not exposed to a webhook.
pub async fn find_by_owner_name(
    db: &Db,
    owner: &str,
    name: &str,
) -> Result<Option<Repo>, QueueError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!("SELECT {COLUMNS} FROM {schema}.repos WHERE owner = $1 AND name = $2");

    let row = pool
        .query_opt(&sql, &[&owner as &dyn quench_pg::ToSql, &name])
        .await?;

    row.as_ref().map(from_row).transpose()
}

pub async fn list(db: &Db) -> Result<Vec<Repo>, QueueError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!("SELECT {COLUMNS} FROM {schema}.repos ORDER BY owner, name");

    let rows = pool.query(&sql, &[]).await?;

    rows.iter().map(from_row).collect()
}

/// Everything but `provider`, which stays fixed - switching it isn't an edit, it's a different repo.
#[derive(Clone, Debug)]
pub struct RepoUpdate {
    pub owner: String,
    pub name: String,
    pub clone_url: String,
    pub default_branch: String,
    pub project_id: String,
    pub enabled: bool,
}

pub async fn update(db: &Db, id: &str, changes: &RepoUpdate) -> Result<Option<Repo>, QueueError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!(
        "UPDATE {schema}.repos SET owner = $2, name = $3, clone_url = $4, \
         default_branch = $5, project_id = $6, enabled = $7, updated_at = NOW() \
         WHERE id = $1 RETURNING {COLUMNS}"
    );

    let row = pool
        .query_opt(
            &sql,
            &[
                &id as &dyn quench_pg::ToSql,
                &changes.owner,
                &changes.name,
                &changes.clone_url,
                &changes.default_branch,
                &changes.project_id,
                &changes.enabled,
            ],
        )
        .await?;

    row.as_ref().map(from_row).transpose()
}

/// Disabling keeps history but stops triggers - for a repo gone bad, not gone away.
pub async fn set_enabled(db: &Db, id: &str, enabled: bool) -> Result<Option<Repo>, QueueError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!(
        "UPDATE {schema}.repos SET enabled = $2, updated_at = NOW() \
         WHERE id = $1 RETURNING {COLUMNS}"
    );

    let row = pool
        .query_opt(&sql, &[&id as &dyn quench_pg::ToSql, &enabled])
        .await?;

    row.as_ref().map(from_row).transpose()
}

pub async fn delete(db: &Db, id: &str) -> Result<bool, QueueError> {
    let pool = pool(db)?;
    let schema = schema();
    let sql = format!("DELETE FROM {schema}.repos WHERE id = $1");

    let result = pool.execute(&sql, &[&id as &dyn quench_pg::ToSql]).await?;

    Ok(result > 0)
}

const COLUMNS: &str = "id, provider, owner, name, clone_url, default_branch, \
                       registered_by, project_id, enabled, created_at, updated_at";

fn from_row(row: &quench_pg::Row) -> Result<Repo, QueueError> {
    let provider: String = row.get("provider")?;

    Ok(Repo {
        id: row.get("id")?,
        provider: Provider::parse(&provider)
            .ok_or_else(|| QueueError::BadRow(format!("unknown provider '{provider}'")))?,
        owner: row.get("owner")?,
        name: row.get("name")?,
        clone_url: row.get("clone_url")?,
        default_branch: row.get("default_branch")?,
        registered_by: row.get("registered_by")?,
        project_id: row.get("project_id")?,
        enabled: row.get("enabled")?,
        created_at: row.get::<Time, _>("created_at")?,
        updated_at: row.get::<Time, _>("updated_at")?,
    })
}
