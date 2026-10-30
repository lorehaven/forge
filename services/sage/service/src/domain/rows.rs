//! Typed row reads for the raw-SQL paths (what `sqlx::query_as` was): rows into structs by column name, or
//! into tuples by position.

use quench_pg::{Error, Pool, ToSql};
use quench_serde::Deserialize;

/// Every row, each read into a struct by column name.
pub async fn fetch_all<T: Deserialize>(
    pool: &Pool,
    sql: &str,
    params: &[&dyn ToSql],
) -> Result<Vec<T>, Error> {
    pool.query(sql, params)
        .await?
        .iter()
        .map(quench_pg::Row::deserialize)
        .collect()
}

/// The first row, if any, read into a struct by column name.
pub async fn fetch_optional<T: Deserialize>(
    pool: &Pool,
    sql: &str,
    params: &[&dyn ToSql],
) -> Result<Option<T>, Error> {
    pool.query_opt(sql, params)
        .await?
        .as_ref()
        .map(quench_pg::Row::deserialize)
        .transpose()
}

/// The only row, read into a struct by column name.
pub async fn fetch_one<T: Deserialize>(
    pool: &Pool,
    sql: &str,
    params: &[&dyn ToSql],
) -> Result<T, Error> {
    pool.query_one(sql, params).await?.deserialize()
}

/// Every row, each read into a tuple by position.
pub async fn fetch_all_tuples<T: Deserialize>(
    pool: &Pool,
    sql: &str,
    params: &[&dyn ToSql],
) -> Result<Vec<T>, Error> {
    pool.query(sql, params)
        .await?
        .iter()
        .map(quench_pg::Row::deserialize_tuple)
        .collect()
}

/// The first row, if any, read into a tuple by position.
pub async fn fetch_optional_tuple<T: Deserialize>(
    pool: &Pool,
    sql: &str,
    params: &[&dyn ToSql],
) -> Result<Option<T>, Error> {
    pool.query_opt(sql, params)
        .await?
        .as_ref()
        .map(quench_pg::Row::deserialize_tuple)
        .transpose()
}

/// The only row, read into a tuple by position.
pub async fn fetch_one_tuple<T: Deserialize>(
    pool: &Pool,
    sql: &str,
    params: &[&dyn ToSql],
) -> Result<T, Error> {
    pool.query_one(sql, params).await?.deserialize_tuple()
}
