//! Shared pool/schema/error plumbing for the dynamic-storage domain.
//! Raw SQL, not `Crud`: quota checks and ref-counts need a locked read-then-write `Crud` can't express.

use quench_db::prelude::Db;
use quench_pg::Pool;

#[derive(Debug, quench_core::Error)]
pub enum StorageError {
    #[error(
        "dynamic storages need Postgres; this service is running against an \
         in-memory database, where every storage and file would be lost on restart"
    )]
    NotPostgres,

    #[error("storage `{0}` does not exist")]
    NoSuchStorage(String),

    #[error("storage quota exceeded")]
    QuotaExceeded,

    #[error(transparent)]
    Sql(#[from] quench_pg::Error),
}

impl StorageError {
    /// True for Postgres's unique-constraint violation (duplicate storage name).
    #[must_use]
    pub fn is_unique_violation(&self) -> bool {
        matches!(self, Self::Sql(error) if error.code() == Some("23505"))
    }

    /// A value the database cannot store because of what it is: a NUL byte, text that is not valid UTF-8, a number out of
    /// range, a string longer than the column or its index allows (SQLSTATE class 22, or 54000). The caller's fault.
    #[must_use]
    pub fn is_unstorable_value(&self) -> bool {
        matches!(self, Self::Sql(error) if error.code().is_some_and(|c| c.starts_with("22") || c == "54000"))
    }

    /// True for Postgres's foreign-key violation (`owner` not a real username).
    #[must_use]
    pub fn is_foreign_key_violation(&self) -> bool {
        matches!(self, Self::Sql(error) if error.code() == Some("23503"))
    }
}

/// The pool, or a clear refusal - an in-memory `Db` would silently lose everything on restart.
pub const fn pool(db: &Db) -> Result<&Pool, StorageError> {
    match db {
        Db::Postgres(postgres) => Ok(postgres.pool()),
        Db::InMemory(_) => Err(StorageError::NotPostgres),
    }
}

#[must_use]
pub fn schema() -> String {
    quench_core::env::get_or("DB_SCHEMA", "warehouse")
}

/// What to tell a caller about a failure whose detail may quote the database.
///
/// A value the database cannot store (SQLSTATE class 22, or 54000) is the caller's fault: 400. Anything else is logged and
/// answered with a generic 500, so a column, a constraint or a SQLSTATE never reaches the client.
#[must_use]
pub fn failure(detail: &str) -> (quench_http::prelude::http::StatusCode, String) {
    use quench_http::prelude::http::StatusCode;
    if detail.contains("SQLSTATE 22") || detail.contains("SQLSTATE 54000") {
        return (
            StatusCode::BAD_REQUEST,
            "a value in the request cannot be stored: it is too long, out of range, or holds a character that is not allowed"
                .to_string(),
        );
    }
    // A foreign key: the account making the request, or a record it names, is not in the realm.
    if detail.contains("SQLSTATE 23503") {
        return (
            StatusCode::BAD_REQUEST,
            "the account making this request, or a record it names, does not exist".to_string(),
        );
    }
    quench_log::error!("warehouse: {detail}");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal error".to_string(),
    )
}
