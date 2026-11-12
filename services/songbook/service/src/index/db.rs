//! The connection the index code talks to: one Postgres connection checked out of the pool, with the shape the SQLite engine
//! had (SQL with `?` marks, `Value` parameters, rows of `Value`s), so the code that reads and writes the tables stays as it was.
//!
//! The pool's connections are opened with `search_path` set to the service's schema, so SQL names tables without a schema.
//! Parameters travel as text and the server infers their type from the statement; rows come back by column type.

use quench_pg::{Pool, PooledConnection, ToSql};
use std::borrow::Cow;

/// What can go wrong with the database.
#[derive(Debug, quench_core::Error)]
pub enum DbError {
    #[error(transparent)]
    Pg(#[from] quench_pg::Error),
    /// Anything else (a task that failed, a file that could not be read).
    #[error("{0}")]
    Unsupported(String),
}

impl DbError {
    /// Postgres's unique-constraint violation (a name or key that is taken).
    #[must_use]
    pub fn is_unique_violation(&self) -> bool {
        matches!(self, Self::Pg(e) if e.code() == Some("23505"))
    }
}

pub type Result<T> = std::result::Result<T, DbError>;

/// One cell of a row, or one parameter.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Integer(i64),
    Real(f64),
    Text(String),
}

impl Value {
    #[must_use]
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(s) => Some(s),
            _ => None,
        }
    }

    #[must_use]
    pub const fn as_integer(&self) -> Option<i64> {
        match self {
            Self::Integer(i) => Some(*i),
            _ => None,
        }
    }

    #[must_use]
    pub const fn as_real(&self) -> Option<f64> {
        match self {
            Self::Real(f) => Some(*f),
            Self::Integer(i) => Some(*i as f64),
            _ => None,
        }
    }

    #[must_use]
    pub const fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }
}

impl ToSql for Value {
    fn to_sql(&self) -> Option<String> {
        match self {
            Self::Null => None,
            Self::Integer(i) => i.to_sql(),
            Self::Real(f) => f.to_sql(),
            Self::Text(s) => Some(s.clone()),
        }
    }
}

impl From<quench_serde::Value> for Value {
    fn from(v: quench_serde::Value) -> Self {
        use quench_serde::Value as V;
        match v {
            V::Null => Self::Null,
            V::Bool(b) => Self::Integer(i64::from(b)),
            V::Int(i) => Self::Integer(i),
            V::UInt(u) => i64::try_from(u).map_or(Self::Real(u as f64), Self::Integer),
            V::Float(f) => Self::Real(f),
            V::String(s) => Self::Text(s),
            other => Self::Text(other.to_string()),
        }
    }
}

/// The rows a query returned.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Rows {
    pub rows: Vec<Vec<Value>>,
}

/// `?` marks to `$1, $2, ...`. A `?` inside a quoted string stays.
#[must_use]
pub fn numbered(sql: &str) -> Cow<'_, str> {
    if !sql.contains('?') {
        return Cow::Borrowed(sql);
    }
    let mut out = String::with_capacity(sql.len() + 8);
    let (mut n, mut quoted) = (0, false);
    for c in sql.chars() {
        match c {
            '\'' => {
                quoted = !quoted;
                out.push(c);
            }
            '?' if !quoted => {
                n += 1;
                out.push('$');
                out.push_str(&n.to_string());
            }
            _ => out.push(c),
        }
    }
    Cow::Owned(out)
}

/// A pooled connection. It goes back to the pool when dropped; one dropped inside a transaction is discarded (the server
/// rolls the transaction back).
pub struct Db {
    conn: PooledConnection,
}

impl std::fmt::Debug for Db {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Db { .. }")
    }
}

impl Db {
    /// Checks a connection out of `pool`.
    pub async fn checkout(pool: &Pool) -> Result<Self> {
        Ok(Self {
            conn: pool.get().await?,
        })
    }

    fn refs(params: &[Value]) -> Vec<&dyn ToSql> {
        params.iter().map(|p| p as &dyn ToSql).collect()
    }

    pub async fn query(&mut self, sql: &str, params: &[Value]) -> Result<Rows> {
        let found = self.conn.query(&numbered(sql), &Self::refs(params)).await?;
        let rows = found
            .iter()
            .map(|row| {
                let cells = row.columns().len();
                (0..cells)
                    .map(|i| row.value(&i).map(Value::from))
                    .collect::<std::result::Result<Vec<_>, _>>()
            })
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(Rows { rows })
    }

    /// Runs a statement; returns how many rows it changed.
    pub async fn execute(&mut self, sql: &str, params: &[Value]) -> Result<u64> {
        Ok(self
            .conn
            .execute(&numbered(sql), &Self::refs(params))
            .await?)
    }

    /// Runs an `INSERT` and returns the `id` of the new row.
    pub async fn insert(&mut self, sql: &str, params: &[Value]) -> Result<i64> {
        let sql = format!("{sql} RETURNING id");
        let rows = self.query(&sql, params).await?;
        rows.rows
            .first()
            .and_then(|r| r.first())
            .and_then(Value::as_integer)
            .ok_or_else(|| DbError::Unsupported("the insert returned no id".into()))
    }

    /// Runs several statements separated by `;` (no parameters).
    pub async fn batch(&mut self, sql: &str) -> Result<()> {
        Ok(self.conn.batch_execute(sql).await?)
    }

    pub async fn begin(&mut self) -> Result<()> {
        self.batch("BEGIN").await
    }

    pub async fn commit(&mut self) -> Result<()> {
        self.batch("COMMIT").await
    }

    /// Runs `f` in a transaction: committed when it returns `Ok` (whatever the `Ok` holds), rolled back when it returns `Err`.
    pub async fn transaction<T>(
        &mut self,
        f: impl AsyncFnOnce(&mut Self) -> Result<T>,
    ) -> Result<T> {
        self.begin().await?;
        match f(self).await {
            Ok(v) => {
                self.commit().await?;
                Ok(v)
            }
            Err(e) => {
                self.rollback().await;
                Err(e)
            }
        }
    }

    /// Ends a transaction that failed; the error that made it fail is the one to report, so this one is not.
    pub async fn rollback(&mut self) {
        let _ = self.batch("ROLLBACK").await;
    }
}

/// Opens the pool for `url` with the schema on the search path.
pub async fn connect(url: &str, schema: &str) -> Result<Pool> {
    let mut config = quench_pg::Config::from_url(url)?;
    config.options = Some(format!("-c search_path={schema}"));
    Ok(Pool::connect_with(config, quench_pg::PoolOptions::default()).await?)
}
