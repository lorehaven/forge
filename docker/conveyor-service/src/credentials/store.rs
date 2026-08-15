//! Reading and writing sealed git credentials.
//!
//! Same discipline as [`crate::secrets::store`]: [`CredentialRef`] is what
//! listing and the API return, and it never carries the token. Getting the
//! token is [`resolve`], called only from the checkout path with the
//! credential key, never exposed as an endpoint.
//!
//! Unlike a pipeline secret, a credential is not looked up by name: a
//! checkout has exactly one clone url, so there is at most one credential per
//! repository or per project, full stop. `name` is kept as a label for
//! listings and audit trails, not as part of a row's identity - the scope
//! (which project, or which repo) is the whole identity, so `put` replaces
//! whatever was in that scope regardless of what it was called.

use crate::domain::Repo;
use crate::scheduler::projects;
use crate::scheduler::queue::{QueueError, pool, schema};
use crate::secrets::crypto::{CryptoError, SecretKey};
use chrono::{DateTime, Utc};
use quench_db::prelude::Db;
use serde::Serialize;
use sqlx::Row;
use uuid::Uuid;

/// The variable a credential's cipher key is read from. Deliberately its own
/// name, not `CONVEYOR_SECRET_KEY` - see the module doc comment.
pub const KEY_VAR: &str = "CONVEYOR_CREDENTIAL_KEY";

/// The shortest token worth storing. Below this a masked preview reveals as
/// much as it hides, the same reasoning `secrets::redact::MIN_REDACTABLE`
/// gives for pipeline secrets.
const MIN_TOKEN_LEN: usize = 4;

#[derive(Debug, thiserror::Error)]
pub enum CredentialError {
    #[error(transparent)]
    Crypto(#[from] CryptoError),

    #[error(transparent)]
    Queue(#[from] QueueError),

    #[error("credential names hold letters, digits and underscores: '{name}' does not")]
    BadName { name: String },

    #[error("a username or token cannot contain a newline or other control character")]
    BadMaterial,

    #[error("a token shorter than {MIN_TOKEN_LEN} characters is not worth storing encrypted")]
    TooShort,
}

/// What a credential belongs to. Unlike [`crate::secrets::store::Scope`],
/// neither side is optional - a credential with nothing to scope it to would
/// apply to every repository conveyor builds, which is not a thing this
/// estate has asked for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Scope {
    Project(String),
    Repo(String),
}

impl Scope {
    fn project_id(&self) -> Option<&str> {
        match self {
            Self::Project(id) => Some(id),
            Self::Repo(_) => None,
        }
    }

    fn repo_id(&self) -> Option<&str> {
        match self {
            Self::Repo(id) => Some(id),
            Self::Project(_) => None,
        }
    }

    /// What the token is sealed against, so a row moved from one project or
    /// repository to another fails to open rather than quietly granting a
    /// credential to something it was never given to.
    fn context(&self) -> String {
        match self {
            Self::Project(id) => format!("project:{id}"),
            Self::Repo(id) => format!("repo:{id}"),
        }
    }
}

/// A credential, without its token.
#[derive(Clone, Debug, Serialize)]
pub struct CredentialRef {
    pub id: String,
    pub project_id: Option<String>,
    pub repo_id: Option<String>,
    pub name: String,
    pub kind: String,
    pub username: String,
    /// A masked fragment, e.g. `"••••…9f2a"` - enough to recognise which
    /// token this is without being able to reconstruct it.
    pub preview: String,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A decrypted credential, held only for the duration of a checkout.
pub struct ResolvedCredential {
    pub username: String,
    pub token: String,
}

/// What a caller supplies to [`put`]. Grouped into one argument rather than
/// four so the function stays under clippy's argument-count lint - these four
/// are exactly "what's being written," as distinct from `db`/`key`/`scope`/
/// `created_by`, which are the context it's written through.
pub struct NewCredential<'a> {
    pub name: &'a str,
    pub kind: &'a str,
    pub username: &'a str,
    pub token: &'a str,
}

/// Names that read sensibly as a label. Not looked up by, so nothing forces
/// this - but reusing `secrets::store::validate_name`'s rule beats inventing
/// a second one.
fn validate_name(name: &str) -> Result<(), CredentialError> {
    let usable = !name.is_empty()
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !name.starts_with(|c: char| c.is_ascii_digit());

    usable
        .then_some(())
        .ok_or_else(|| CredentialError::BadName {
            name: name.to_string(),
        })
}

/// Both `username` and `token` end up in an HTTP header value
/// (`workspace::checkout` builds `Authorization: Basic ...` from them), so a
/// carriage return or newline here would let a stored credential inject a
/// second header into every request that uses it.
fn validate_material(username: &str, token: &str) -> Result<(), CredentialError> {
    let clean = |s: &str| !s.chars().any(|c| c.is_control());
    if !clean(username) || !clean(token) {
        return Err(CredentialError::BadMaterial);
    }
    if token.chars().count() < MIN_TOKEN_LEN {
        return Err(CredentialError::TooShort);
    }
    Ok(())
}

fn preview_of(token: &str) -> String {
    let tail: String = token
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("••••…{tail}")
}

/// Writes the credential for `scope`, replacing whatever was there under any
/// name.
pub async fn put(
    db: &Db,
    key: &SecretKey,
    scope: &Scope,
    new: &NewCredential<'_>,
    created_by: &str,
) -> Result<CredentialRef, CredentialError> {
    let NewCredential {
        name,
        kind,
        username,
        token,
    } = *new;

    validate_name(name)?;
    validate_material(username, token)?;

    let (nonce, ciphertext) = key.seal(&scope.context(), token)?;
    let preview = preview_of(token);
    let pool = pool(db).map_err(CredentialError::Queue)?;
    let schema = schema();

    // One partial unique index per scope column - at most one credential per
    // project, at most one per repo, `name` playing no part in identity.
    let sql = match scope {
        Scope::Project(_) => format!(
            "INSERT INTO {schema}.credentials \
             (id, project_id, repo_id, name, kind, username, preview, nonce, ciphertext, created_by) \
             VALUES ($1, $2, NULL, $3, $4, $5, $6, $7, $8, $9) \
             ON CONFLICT (project_id) WHERE project_id IS NOT NULL DO UPDATE SET \
                 name = EXCLUDED.name, kind = EXCLUDED.kind, username = EXCLUDED.username, \
                 preview = EXCLUDED.preview, nonce = EXCLUDED.nonce, \
                 ciphertext = EXCLUDED.ciphertext, updated_at = NOW() \
             RETURNING {COLUMNS}"
        ),
        Scope::Repo(_) => format!(
            "INSERT INTO {schema}.credentials \
             (id, project_id, repo_id, name, kind, username, preview, nonce, ciphertext, created_by) \
             VALUES ($1, NULL, $2, $3, $4, $5, $6, $7, $8, $9) \
             ON CONFLICT (repo_id) WHERE repo_id IS NOT NULL DO UPDATE SET \
                 name = EXCLUDED.name, kind = EXCLUDED.kind, username = EXCLUDED.username, \
                 preview = EXCLUDED.preview, nonce = EXCLUDED.nonce, \
                 ciphertext = EXCLUDED.ciphertext, updated_at = NOW() \
             RETURNING {COLUMNS}"
        ),
    };

    let scope_id = match scope {
        Scope::Project(id) | Scope::Repo(id) => id,
    };

    let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(Uuid::new_v4().to_string())
        .bind(scope_id)
        .bind(name)
        .bind(kind)
        .bind(username)
        .bind(&preview)
        .bind(&nonce)
        .bind(&ciphertext)
        .bind(created_by)
        .fetch_one(pool)
        .await
        .map_err(QueueError::from)?;

    Ok(from_row(&row)?)
}

/// The credential in `scope`, without its token - `None` when nothing is set.
pub async fn show(db: &Db, scope: &Scope) -> Result<Option<CredentialRef>, CredentialError> {
    let pool = pool(db).map_err(CredentialError::Queue)?;
    let schema = schema();

    let sql = format!(
        "SELECT {COLUMNS} FROM {schema}.credentials \
         WHERE project_id IS NOT DISTINCT FROM $1 AND repo_id IS NOT DISTINCT FROM $2"
    );

    let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(scope.project_id())
        .bind(scope.repo_id())
        .fetch_optional(pool)
        .await
        .map_err(QueueError::from)?;

    row.as_ref().map(from_row).transpose().map_err(Into::into)
}

/// Every credential, across every scope - callers filter by what the caller
/// may read. Used only by the UI preview page; there is still no way to
/// read a token back out through this, only the same `preview` `show` and
/// the API already expose.
pub async fn list_all(db: &Db) -> Result<Vec<CredentialRef>, CredentialError> {
    let pool = pool(db).map_err(CredentialError::Queue)?;
    let schema = schema();

    let sql = format!("SELECT {COLUMNS} FROM {schema}.credentials ORDER BY name");

    let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .fetch_all(pool)
        .await
        .map_err(QueueError::from)?;

    rows.iter().map(|row| Ok(from_row(row)?)).collect()
}

pub async fn delete(db: &Db, scope: &Scope) -> Result<bool, CredentialError> {
    let pool = pool(db).map_err(CredentialError::Queue)?;
    let schema = schema();

    let sql = format!(
        "DELETE FROM {schema}.credentials \
         WHERE project_id IS NOT DISTINCT FROM $1 AND repo_id IS NOT DISTINCT FROM $2"
    );

    let result = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(scope.project_id())
        .bind(scope.repo_id())
        .execute(pool)
        .await
        .map_err(QueueError::from)?;

    Ok(result.rows_affected() > 0)
}

/// The token in `scope`, decrypted. Used by `resolve` and nowhere else -
/// there is no API path that reaches this.
async fn material(
    db: &Db,
    key: &SecretKey,
    scope: &Scope,
) -> Result<Option<ResolvedCredential>, CredentialError> {
    let pool = pool(db).map_err(CredentialError::Queue)?;
    let schema = schema();

    let sql = format!(
        "SELECT username, nonce, ciphertext FROM {schema}.credentials \
         WHERE project_id IS NOT DISTINCT FROM $1 AND repo_id IS NOT DISTINCT FROM $2"
    );

    let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(scope.project_id())
        .bind(scope.repo_id())
        .fetch_optional(pool)
        .await
        .map_err(QueueError::from)?;

    let Some(row) = row else {
        return Ok(None);
    };

    let username: String = row.try_get("username").map_err(QueueError::from)?;
    let nonce: Vec<u8> = row.try_get("nonce").map_err(QueueError::from)?;
    let ciphertext: Vec<u8> = row.try_get("ciphertext").map_err(QueueError::from)?;
    let token = key.open(&scope.context(), &nonce, &ciphertext)?;

    Ok(Some(ResolvedCredential { username, token }))
}

/// The credential a checkout of `repo` should authenticate with, if any.
///
/// A credential registered directly on the repository wins. Otherwise this
/// walks from the repo's project up through its ancestors, nearest first,
/// and uses the first one found - so a token set on a parent project covers
/// every repository under it, and a more specific one lower in the tree can
/// still override it. `None` means try the clone unauthenticated, exactly
/// today's behaviour: a repo nobody has given a credential is assumed public.
///
/// The walk goes level by level via `projects::read` rather than through
/// `projects::ancestor_chain`: that helper's rows are explicitly unordered
/// (callers only ever check membership), while resolution here needs
/// deterministic nearest-wins precedence.
pub async fn resolve(
    db: &Db,
    key: Option<&SecretKey>,
    repo: &Repo,
) -> Result<Option<ResolvedCredential>, CredentialError> {
    let Some(key) = key else {
        return Ok(None);
    };

    if let Some(credential) = material(db, key, &Scope::Repo(repo.id.clone())).await? {
        return Ok(Some(credential));
    }

    let mut project_id = Some(repo.project_id.clone());
    // Bounds the walk against a corrupted tree; a real one is never this
    // deep, so this never fires in practice.
    for _ in 0..64 {
        let Some(id) = project_id else { break };

        if let Some(credential) = material(db, key, &Scope::Project(id.clone())).await? {
            return Ok(Some(credential));
        }

        project_id = projects::read(db, &id)
            .await
            .map_err(CredentialError::Queue)?
            .and_then(|project| project.parent_id);
    }

    Ok(None)
}

const COLUMNS: &str =
    "id, project_id, repo_id, name, kind, username, preview, created_by, created_at, updated_at";

fn from_row(row: &sqlx::postgres::PgRow) -> Result<CredentialRef, QueueError> {
    Ok(CredentialRef {
        id: row.try_get("id")?,
        project_id: row.try_get("project_id")?,
        repo_id: row.try_get("repo_id")?,
        name: row.try_get("name")?,
        kind: row.try_get("kind")?,
        username: row.try_get("username")?,
        preview: row.try_get("preview")?,
        created_by: row.try_get("created_by")?,
        created_at: row.try_get::<DateTime<Utc>, _>("created_at")?,
        updated_at: row.try_get::<DateTime<Utc>, _>("updated_at")?,
    })
}
