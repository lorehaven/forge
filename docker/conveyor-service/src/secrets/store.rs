//! Reading and writing sealed values.
//!
//! The store never hands back a `Secret` with its value attached by accident:
//! [`SecretRef`] is what listing returns and what the API serialises, and
//! getting the value is a separate call that needs the key.

use crate::scheduler::queue::{QueueError, pool, schema};
use crate::secrets::crypto::{CryptoError, SecretKey};
use crate::secrets::redact::MIN_REDACTABLE;
use chrono::{DateTime, Utc};
use quench_db::prelude::Db;
use serde::Serialize;
use sqlx::Row;
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    #[error(transparent)]
    Crypto(#[from] CryptoError),

    #[error(transparent)]
    Queue(#[from] QueueError),

    #[error("secret names hold letters, digits and underscores: '{name}' does not")]
    BadName { name: String },

    #[error(
        "a secret shorter than {MIN_REDACTABLE} characters cannot be kept out of \
         build logs, so conveyor will not store one"
    )]
    TooShort,

    #[error("this pipeline needs secrets that are not set: {}", .names.join(", "))]
    Missing { names: Vec<String> },
}

/// Who a secret belongs to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Scope {
    /// Available to every pipeline.
    Global,
    /// Available only to the named repository's pipelines.
    Repo(String),
}

impl Scope {
    pub fn repo_id(&self) -> Option<&str> {
        match self {
            Self::Global => None,
            Self::Repo(id) => Some(id),
        }
    }

    /// What the value is sealed against, so a row moved between scopes or
    /// renamed in place fails to open.
    fn context(&self, name: &str) -> String {
        match self {
            Self::Global => format!("global:{name}"),
            Self::Repo(id) => format!("repo:{id}:{name}"),
        }
    }
}

/// A secret, without its value.
#[derive(Clone, Debug, Serialize)]
pub struct SecretRef {
    pub id: String,
    pub repo_id: Option<String>,
    pub name: String,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Names that can become environment variables without surprising anyone.
fn validate_name(name: &str) -> Result<(), SecretError> {
    let usable = !name.is_empty()
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !name.starts_with(|c: char| c.is_ascii_digit());

    usable.then_some(()).ok_or_else(|| SecretError::BadName {
        name: name.to_string(),
    })
}

/// Writes a secret, replacing whatever was there under the same name.
pub async fn put(
    db: &Db,
    key: &SecretKey,
    scope: &Scope,
    name: &str,
    value: &str,
    created_by: &str,
) -> Result<SecretRef, SecretError> {
    validate_name(name)?;
    if value.chars().count() < MIN_REDACTABLE {
        return Err(SecretError::TooShort);
    }

    let (nonce, ciphertext) = key.seal(&scope.context(name), value)?;
    let pool = pool(db).map_err(SecretError::Queue)?;
    let schema = schema();

    // Two partial unique indexes rather than one, because Postgres treats NULLs
    // as distinct - so the conflict target differs by scope and the two cases
    // cannot share a statement.
    let sql = match scope {
        Scope::Global => format!(
            "INSERT INTO {schema}.secrets (id, repo_id, name, nonce, ciphertext, created_by) \
             VALUES ($1, NULL, $2, $3, $4, $5) \
             ON CONFLICT (name) WHERE repo_id IS NULL DO UPDATE SET \
                 nonce = EXCLUDED.nonce, ciphertext = EXCLUDED.ciphertext, updated_at = NOW() \
             RETURNING {COLUMNS}"
        ),
        Scope::Repo(_) => format!(
            "INSERT INTO {schema}.secrets (id, repo_id, name, nonce, ciphertext, created_by) \
             VALUES ($1, $6, $2, $3, $4, $5) \
             ON CONFLICT (repo_id, name) WHERE repo_id IS NOT NULL DO UPDATE SET \
                 nonce = EXCLUDED.nonce, ciphertext = EXCLUDED.ciphertext, updated_at = NOW() \
             RETURNING {COLUMNS}"
        ),
    };

    let mut query = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(Uuid::new_v4().to_string())
        .bind(name)
        .bind(&nonce)
        .bind(&ciphertext)
        .bind(created_by);
    if let Scope::Repo(id) = scope {
        query = query.bind(id);
    }

    let row = query.fetch_one(pool).await.map_err(QueueError::from)?;
    Ok(from_row(&row)?)
}

/// The value, or `None` when nothing is stored under that name in that scope.
pub async fn get(
    db: &Db,
    key: &SecretKey,
    scope: &Scope,
    name: &str,
) -> Result<Option<String>, SecretError> {
    let pool = pool(db).map_err(SecretError::Queue)?;
    let schema = schema();

    let sql = format!(
        "SELECT nonce, ciphertext FROM {schema}.secrets \
         WHERE name = $1 AND repo_id IS NOT DISTINCT FROM $2"
    );

    let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(name)
        .bind(scope.repo_id())
        .fetch_optional(pool)
        .await
        .map_err(QueueError::from)?;

    let Some(row) = row else {
        return Ok(None);
    };

    let nonce: Vec<u8> = row.try_get("nonce").map_err(QueueError::from)?;
    let ciphertext: Vec<u8> = row.try_get("ciphertext").map_err(QueueError::from)?;
    Ok(Some(key.open(&scope.context(name), &nonce, &ciphertext)?))
}

/// What a job asked for, looked up in the order that lets a repository
/// override the estate.
///
/// A declared secret that is set nowhere is an error rather than an empty
/// string: a deploy step that runs with a blank token fails somewhere further
/// on, in a way that takes much longer to understand.
pub async fn resolve(
    db: &Db,
    key: Option<&SecretKey>,
    repo_id: &str,
    names: &[String],
) -> Result<BTreeMap<String, String>, SecretError> {
    if names.is_empty() {
        return Ok(BTreeMap::new());
    }

    let key = key.ok_or(CryptoError::NoKey)?;
    let repo_scope = Scope::Repo(repo_id.to_string());

    let mut resolved = BTreeMap::new();
    let mut missing = Vec::new();

    for name in names {
        // The repository's own value first, then the estate's. That way a
        // shared default can be set once and overridden where it matters.
        let value = match get(db, key, &repo_scope, name).await? {
            Some(value) => Some(value),
            None => get(db, key, &Scope::Global, name).await?,
        };

        match value {
            Some(value) => {
                resolved.insert(name.clone(), value);
            }
            None => missing.push(name.clone()),
        }
    }

    if !missing.is_empty() {
        return Err(SecretError::Missing { names: missing });
    }

    Ok(resolved)
}

/// Every secret in a scope, without values.
pub async fn list(db: &Db, scope: &Scope) -> Result<Vec<SecretRef>, SecretError> {
    let pool = pool(db).map_err(SecretError::Queue)?;
    let schema = schema();

    let sql = format!(
        "SELECT {COLUMNS} FROM {schema}.secrets \
         WHERE repo_id IS NOT DISTINCT FROM $1 ORDER BY name"
    );

    let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(scope.repo_id())
        .fetch_all(pool)
        .await
        .map_err(QueueError::from)?;

    rows.iter().map(|row| Ok(from_row(row)?)).collect()
}

pub async fn delete(db: &Db, scope: &Scope, name: &str) -> Result<bool, SecretError> {
    let pool = pool(db).map_err(SecretError::Queue)?;
    let schema = schema();

    let sql =
        format!("DELETE FROM {schema}.secrets WHERE name = $1 AND repo_id IS NOT DISTINCT FROM $2");

    let result = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(name)
        .bind(scope.repo_id())
        .execute(pool)
        .await
        .map_err(QueueError::from)?;

    Ok(result.rows_affected() > 0)
}

const COLUMNS: &str = "id, repo_id, name, created_by, created_at, updated_at";

fn from_row(row: &sqlx::postgres::PgRow) -> Result<SecretRef, QueueError> {
    Ok(SecretRef {
        id: row.try_get("id")?,
        repo_id: row.try_get("repo_id")?,
        name: row.try_get("name")?,
        created_by: row.try_get("created_by")?,
        created_at: row.try_get::<DateTime<Utc>, _>("created_at")?,
        updated_at: row.try_get::<DateTime<Utc>, _>("updated_at")?,
    })
}
