//! The merged catalogue in Postgres (the `hoist` schema, installed by foundry's `hoist` module).
//!
//! Every refresh rewrites the table inside one transaction: readers never see a half-merged catalogue, and a run
//! that fails part way leaves the previous hour's data in place.

use quench_core::Error;
use quench_db::prelude::Db;
use quench_pg::{Pool, ToSql};

use crate::models::MergedPackage;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("the catalogue needs Postgres; the in-memory database does not keep it")]
    NotPostgres,
    #[error(transparent)]
    Sql(#[from] quench_pg::Error),
}

/// The schema foundry installs the catalogue in.
#[must_use]
pub fn schema() -> String {
    quench_core::env::get_or("DB_SCHEMA", "hoist")
}

fn pool(db: &Db) -> Result<&Pool, StoreError> {
    match db {
        Db::Postgres(postgres) => Ok(postgres.pool()),
        Db::InMemory(_) => Err(StoreError::NotPostgres),
    }
}

/// How a page is ordered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sort {
    Downloads,
    Rating,
    Alpha,
}

impl Sort {
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "downloads" => Some(Self::Downloads),
            "rating" => Some(Self::Rating),
            "alpha" => Some(Self::Alpha),
            _ => None,
        }
    }

    /// Postgres sorts NULL first when descending: what has no figures goes last, not first.
    const fn order_by(self) -> &'static str {
        match self {
            Self::Downloads => "downloads DESC NULLS LAST, label",
            Self::Rating => "rating_avg DESC NULLS LAST, rating_count DESC NULLS LAST, label",
            Self::Alpha => "LOWER(label), package_name",
        }
    }
}

const COLUMNS: &str = "package_name, label, summary, description, icon_url, category, version_code, version_name, \
    size_bytes, download_url, sha256, md5, install_source, rating_avg, rating_count, downloads, malware_rank";
const FIELDS: usize = 17;
/// Rows per INSERT: 17 parameters each stays far under Postgres's 65535.
const CHUNK: usize = 200;

/// Replaces the whole catalogue with `packages`, in one transaction.
///
/// # Errors
/// [`StoreError`]; nothing is changed then.
pub async fn replace_all(db: &Db, packages: &[MergedPackage]) -> Result<(), StoreError> {
    let schema = schema();
    let mut conn = pool(db)?.get().await?;
    let mut tx = conn.begin().await?;
    tx.execute(&format!("DELETE FROM {schema}.packages"), &[])
        .await?;
    for chunk in packages.chunks(CHUNK) {
        let mut sql = format!("INSERT INTO {schema}.packages ({COLUMNS}) VALUES ");
        let mut params: Vec<&dyn ToSql> = Vec::with_capacity(chunk.len() * FIELDS);
        for (i, p) in chunk.iter().enumerate() {
            if i > 0 {
                sql.push(',');
            }
            sql.push('(');
            for f in 0..FIELDS {
                if f > 0 {
                    sql.push(',');
                }
                sql.push_str(&format!("${}", i * FIELDS + f + 1));
            }
            sql.push(')');
            params.extend_from_slice(&[
                &p.package_name,
                &p.label,
                &p.summary,
                &p.description,
                &p.icon_url,
                &p.category,
                &p.version_code,
                &p.version_name,
                &p.size_bytes,
                &p.download_url,
                &p.sha256,
                &p.md5,
                &p.install_source,
                &p.rating_avg,
                &p.rating_count,
                &p.downloads,
                &p.malware_rank,
            ]);
        }
        tx.execute(&sql, &params).await?;
    }
    tx.commit().await?;
    Ok(())
}

fn from_row(row: &quench_pg::Row) -> Result<MergedPackage, StoreError> {
    Ok(MergedPackage {
        package_name: row.get("package_name")?,
        label: row.get("label")?,
        summary: row.get("summary")?,
        description: row.get("description")?,
        icon_url: row.get("icon_url")?,
        category: row.get("category")?,
        version_code: row.get("version_code")?,
        version_name: row.get("version_name")?,
        size_bytes: row.get("size_bytes")?,
        download_url: row.get("download_url")?,
        sha256: row.get("sha256")?,
        md5: row.get("md5")?,
        install_source: row.get("install_source")?,
        rating_avg: row.get("rating_avg")?,
        rating_count: row.get("rating_count")?,
        downloads: row.get("downloads")?,
        malware_rank: row.get("malware_rank")?,
    })
}

/// What a page of the catalogue is asked to hold.
#[derive(Debug, Clone, Default)]
pub struct Filter<'a> {
    /// Part of the label or the package name, any case.
    pub query: Option<&'a str>,
    pub category: Option<&'a str>,
    /// The fewest ratings an app must have.
    pub min_votes: Option<i64>,
}

/// `%` and `_` in what a person typed are letters, not wildcards.
fn like(text: &str) -> String {
    let mut out = String::from("%");
    for c in text.chars() {
        if matches!(c, '\\' | '%' | '_') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('%');
    out
}

/// A page of the catalogue and how many apps match in all.
///
/// # Errors
/// [`StoreError`].
pub async fn search(
    db: &Db,
    filter: &Filter<'_>,
    sort: Sort,
    offset: i64,
    limit: i64,
) -> Result<(Vec<MergedPackage>, i64), StoreError> {
    let schema = schema();
    let pattern = filter.query.map(like);
    let mut clauses: Vec<String> = Vec::new();
    let mut params: Vec<&dyn ToSql> = Vec::new();
    if let Some(p) = &pattern {
        params.push(p);
        clauses.push(format!(
            "(label ILIKE ${0} OR package_name ILIKE ${0})",
            params.len()
        ));
    }
    if let Some(c) = &filter.category {
        params.push(c);
        clauses.push(format!("category = ${}", params.len()));
    }
    if let Some(v) = &filter.min_votes {
        params.push(v);
        clauses.push(format!("rating_count >= ${}", params.len()));
    }
    let where_sql = if clauses.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", clauses.join(" AND "))
    };
    let pool = pool(db)?;
    let total: i64 = pool
        .query_one(
            &format!("SELECT COUNT(*) AS n FROM {schema}.packages {where_sql}"),
            &params,
        )
        .await?
        .get("n")?;
    let (limit, offset) = (limit.clamp(1, 100), offset.max(0));
    params.push(&limit);
    let limit_at = params.len();
    params.push(&offset);
    let rows = pool
        .query(
            &format!(
                "SELECT {COLUMNS} FROM {schema}.packages {where_sql} ORDER BY {} LIMIT ${limit_at} OFFSET ${}",
                sort.order_by(),
                params.len()
            ),
            &params,
        )
        .await?;
    Ok((rows.iter().map(from_row).collect::<Result<_, _>>()?, total))
}

/// One package.
///
/// # Errors
/// [`StoreError`].
pub async fn get(db: &Db, package_name: &str) -> Result<Option<MergedPackage>, StoreError> {
    let schema = schema();
    let row = pool(db)?
        .query_opt(
            &format!("SELECT {COLUMNS} FROM {schema}.packages WHERE package_name = $1"),
            &[&package_name],
        )
        .await?;
    row.as_ref().map(from_row).transpose()
}

/// Every category with how many apps it holds, the biggest first (an app with none is not counted).
///
/// # Errors
/// [`StoreError`].
pub async fn categories(db: &Db) -> Result<Vec<(String, i64)>, StoreError> {
    let schema = schema();
    let rows = pool(db)?
        .query(
            &format!(
                "SELECT category, COUNT(*) AS n FROM {schema}.packages \
                 WHERE category IS NOT NULL AND category <> '' \
                 GROUP BY category ORDER BY n DESC, LOWER(category)"
            ),
            &[],
        )
        .await?;
    rows.iter()
        .map(|r| Ok((r.get("category")?, r.get("n")?)))
        .collect()
}
