//! Applies a [`MigrationPlan`] to Postgres and records what was done.
//!
//! State lives in a dedicated schema (`foundry` by default) holding two tables,
//! `forge_migrations` and `forge_modules`: one row per applied migration
//! instance, one row per installed module. Keeping them out of `public` means
//! the bookkeeping never collides with, or gets dropped alongside, the schemas
//! it tracks. A session-level advisory lock keeps concurrent runs - two
//! Kubernetes Jobs, a Job racing an init container - from applying the same
//! plan twice.

use crate::backends::postgres::PostgresDb;
use crate::error::DbError;
use crate::plan::{MigrationPlan, PlannedMigration};
use sha2::{Digest, Sha256};
use sqlx::Connection;
use std::collections::HashMap;

/// Dedicated schema holding the ledger tables.
pub const DEFAULT_LEDGER_SCHEMA: &str = "foundry";
pub const DEFAULT_LEDGER_TABLE: &str = "forge_migrations";
pub const DEFAULT_MODULE_TABLE: &str = "forge_modules";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrationOutcome {
    /// Statements were executed and recorded.
    Applied,
    /// Already recorded in the ledger.
    Skipped,
    /// Would be applied - reported by a dry run.
    Pending,
}

#[derive(Debug, Clone)]
pub struct MigrationResult {
    pub id: String,
    pub module: String,
    pub schema: String,
    pub migration_id: String,
    pub outcome: MigrationOutcome,
}

/// What a reset removed.
#[derive(Debug, Clone, Default)]
pub struct ResetReport {
    pub schemas: Vec<String>,
    pub forgotten_migrations: usize,
}

#[derive(Debug, Clone, Default)]
pub struct ApplyReport {
    pub results: Vec<MigrationResult>,
    pub dry_run: bool,
}

impl ApplyReport {
    pub fn count(&self, outcome: MigrationOutcome) -> usize {
        self.results
            .iter()
            .filter(|result| result.outcome == outcome)
            .count()
    }

    pub fn with_outcome(
        &self,
        outcome: MigrationOutcome,
    ) -> impl Iterator<Item = &MigrationResult> {
        self.results
            .iter()
            .filter(move |result| result.outcome == outcome)
    }
}

#[derive(Debug, Clone)]
pub struct MigrationRunner {
    ledger_schema: String,
    ledger_table: String,
    module_table: String,
    allow_drift: bool,
    dry_run: bool,
}

impl Default for MigrationRunner {
    fn default() -> Self {
        Self {
            ledger_schema: DEFAULT_LEDGER_SCHEMA.to_string(),
            ledger_table: DEFAULT_LEDGER_TABLE.to_string(),
            module_table: DEFAULT_MODULE_TABLE.to_string(),
            allow_drift: false,
            dry_run: false,
        }
    }
}

impl MigrationRunner {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn ledger_schema(mut self, schema: impl Into<String>) -> Self {
        self.ledger_schema = schema.into();
        self
    }

    pub fn ledger_table(mut self, table: impl Into<String>) -> Self {
        self.ledger_table = table.into();
        self
    }

    pub fn module_table(mut self, table: impl Into<String>) -> Self {
        self.module_table = table.into();
        self
    }

    /// Accept migrations whose SQL changed after they were applied.
    pub fn allow_drift(mut self, allow: bool) -> Self {
        self.allow_drift = allow;
        self
    }

    /// Report what would happen without writing anything.
    pub fn dry_run(mut self, dry_run: bool) -> Self {
        self.dry_run = dry_run;
        self
    }

    fn qualified(&self, table: &str) -> String {
        format!("{}.{}", self.ledger_schema, table)
    }

    fn validate(&self) -> Result<(), DbError> {
        validate_identifier("ledger schema", &self.ledger_schema)?;
        validate_identifier("ledger table", &self.ledger_table)?;
        validate_identifier("module table", &self.module_table)?;
        Ok(())
    }

    pub async fn apply(
        &self,
        db: &PostgresDb,
        plan: &MigrationPlan,
    ) -> Result<ApplyReport, DbError> {
        self.validate()?;

        let mut conn = db.pool().acquire().await?;
        let lock_key = advisory_lock_key(&self.qualified(&self.ledger_table));

        if !self.dry_run {
            sqlx::query(sqlx::AssertSqlSafe("SELECT pg_advisory_lock($1)"))
                .bind(lock_key)
                .execute(&mut *conn)
                .await?;
        }

        let outcome = self.run(&mut conn, plan).await;

        if !self.dry_run {
            // Session locks outlive the transaction and the pooled connection,
            // so release explicitly on both the happy and the error path.
            sqlx::query(sqlx::AssertSqlSafe("SELECT pg_advisory_unlock($1)"))
                .bind(lock_key)
                .execute(&mut *conn)
                .await
                .ok();
        }

        outcome
    }

    async fn run(
        &self,
        conn: &mut sqlx::PgConnection,
        plan: &MigrationPlan,
    ) -> Result<ApplyReport, DbError> {
        if !self.dry_run {
            self.ensure_tables(conn).await?;
        }

        let applied = self.applied_checksums(conn).await?;

        let mut report = ApplyReport {
            dry_run: self.dry_run,
            ..Default::default()
        };

        for migration in &plan.migrations {
            let outcome = if let Some(recorded) = applied.get(&migration.id) {
                if recorded != &migration.checksum {
                    let message = format!(
                        "migration '{}' changed after it was applied (recorded {}, catalog {})",
                        migration.id, recorded, migration.checksum
                    );
                    if !self.allow_drift {
                        return Err(DbError::Unknown(message));
                    }
                    tracing::warn!("{message}");
                }
                MigrationOutcome::Skipped
            } else if self.dry_run {
                MigrationOutcome::Pending
            } else {
                self.execute(conn, migration).await?;
                MigrationOutcome::Applied
            };

            report.results.push(MigrationResult {
                id: migration.id.clone(),
                module: migration.module.clone(),
                schema: migration.schema.clone(),
                migration_id: migration.migration_id.clone(),
                outcome,
            });
        }

        if !self.dry_run {
            for module in &plan.modules {
                self.record_module(conn, &module.module, &module.schema, &module.version)
                    .await?;
            }
        }

        Ok(report)
    }

    async fn ensure_tables(&self, conn: &mut sqlx::PgConnection) -> Result<(), DbError> {
        sqlx::query(sqlx::AssertSqlSafe(
            format!(
                "CREATE SCHEMA IF NOT EXISTS {schema}",
                schema = self.ledger_schema
            )
            .as_str(),
        ))
        .execute(&mut *conn)
        .await?;

        let statements = [
            format!(
                "CREATE TABLE IF NOT EXISTS {table} (
                    id TEXT PRIMARY KEY,
                    module TEXT NOT NULL,
                    target_schema TEXT NOT NULL,
                    migration_id TEXT NOT NULL,
                    since TEXT NOT NULL,
                    module_version TEXT NOT NULL,
                    author TEXT NOT NULL,
                    checksum TEXT NOT NULL,
                    applied_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
                )",
                table = self.qualified(&self.ledger_table)
            ),
            format!(
                "CREATE TABLE IF NOT EXISTS {table} (
                    module TEXT NOT NULL,
                    target_schema TEXT NOT NULL,
                    version TEXT NOT NULL,
                    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
                    PRIMARY KEY (module, target_schema)
                )",
                table = self.qualified(&self.module_table)
            ),
        ];

        for statement in statements {
            sqlx::query(sqlx::AssertSqlSafe(statement.as_str()))
                .execute(&mut *conn)
                .await?;
        }
        Ok(())
    }

    async fn applied_checksums(
        &self,
        conn: &mut sqlx::PgConnection,
    ) -> Result<HashMap<String, String>, DbError> {
        let table = self.qualified(&self.ledger_table);
        if !table_exists(conn, &table).await? {
            return Ok(HashMap::new());
        }

        let query = format!("SELECT id, checksum FROM {table}");
        let rows: Vec<(String, String)> = sqlx::query_as(sqlx::AssertSqlSafe(query.as_str()))
            .fetch_all(&mut *conn)
            .await?;
        Ok(rows.into_iter().collect())
    }

    async fn execute(
        &self,
        conn: &mut sqlx::PgConnection,
        migration: &PlannedMigration,
    ) -> Result<(), DbError> {
        tracing::info!("applying migration {}", migration.id);
        let mut tx = conn.begin().await?;

        for statement in &migration.statements {
            sqlx::query(sqlx::AssertSqlSafe(statement.as_str()))
                .execute(&mut *tx)
                .await
                .map_err(|e| DbError::QueryError(format!("migration '{}': {e}", migration.id)))?;
        }

        insert_ledger_row(&mut tx, &self.qualified(&self.ledger_table), migration).await?;

        tx.commit().await?;
        Ok(())
    }

    async fn record_module(
        &self,
        conn: &mut sqlx::PgConnection,
        module: &str,
        schema: &str,
        version: &semver::Version,
    ) -> Result<(), DbError> {
        let query = format!(
            "INSERT INTO {table} (module, target_schema, version, updated_at)
             VALUES ($1, $2, $3, CURRENT_TIMESTAMP)
             ON CONFLICT (module, target_schema)
             DO UPDATE SET version = EXCLUDED.version, updated_at = CURRENT_TIMESTAMP",
            table = self.qualified(&self.module_table)
        );
        sqlx::query(sqlx::AssertSqlSafe(query.as_str()))
            .bind(module)
            .bind(schema)
            .bind(version.to_string())
            .execute(&mut *conn)
            .await?;
        Ok(())
    }

    /// Drops everything the plan owns and forgets it was ever applied, so a
    /// following `apply` rebuilds from nothing.
    ///
    /// For development. `public` and the ledger's own schema are never dropped,
    /// and the ledger rows are removed in the same run - leaving them behind is
    /// what makes a dropped schema look "already up to date" forever.
    pub async fn reset(
        &self,
        db: &PostgresDb,
        plan: &MigrationPlan,
    ) -> Result<ResetReport, DbError> {
        self.validate()?;

        let mut conn = db.pool().acquire().await?;
        let lock_key = advisory_lock_key(&self.qualified(&self.ledger_table));
        sqlx::query(sqlx::AssertSqlSafe("SELECT pg_advisory_lock($1)"))
            .bind(lock_key)
            .execute(&mut *conn)
            .await?;

        let outcome = self.run_reset(&mut conn, plan).await;

        sqlx::query(sqlx::AssertSqlSafe("SELECT pg_advisory_unlock($1)"))
            .bind(lock_key)
            .execute(&mut *conn)
            .await
            .ok();

        outcome
    }

    /// Schemas [`MigrationRunner::reset`] would drop, so a caller can show the
    /// same list it will act on.
    ///
    /// Database-scoped modules (extensions) have no schema of their own, and
    /// dropping `public` or the ledger's own schema would take out far more
    /// than this plan owns.
    pub fn resettable_schemas(&self, plan: &MigrationPlan) -> Vec<String> {
        let mut schemas: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for module in &plan.modules {
            if module.scope == crate::catalog::ModuleScope::Schema
                && module.schema != "public"
                && module.schema != self.ledger_schema
            {
                schemas.insert(module.schema.clone());
            }
        }
        schemas.into_iter().collect()
    }

    async fn run_reset(
        &self,
        conn: &mut sqlx::PgConnection,
        plan: &MigrationPlan,
    ) -> Result<ResetReport, DbError> {
        let schemas = self.resettable_schemas(plan);

        for schema in &schemas {
            validate_identifier("target schema", schema)?;
            tracing::warn!("dropping schema {schema}");
            sqlx::query(sqlx::AssertSqlSafe(
                format!("DROP SCHEMA IF EXISTS {schema} CASCADE").as_str(),
            ))
            .execute(&mut *conn)
            .await?;
        }

        let ledger = self.qualified(&self.ledger_table);
        let mut forgotten = 0;
        if table_exists(conn, &ledger).await? {
            let ids: Vec<String> = plan.migrations.iter().map(|m| m.id.clone()).collect();
            let deleted = sqlx::query(sqlx::AssertSqlSafe(
                format!("DELETE FROM {ledger} WHERE id = ANY($1)").as_str(),
            ))
            .bind(&ids)
            .execute(&mut *conn)
            .await?;
            forgotten = deleted.rows_affected() as usize;
        }

        let modules = self.qualified(&self.module_table);
        if table_exists(conn, &modules).await? {
            for module in &plan.modules {
                sqlx::query(sqlx::AssertSqlSafe(
                    format!("DELETE FROM {modules} WHERE module = $1 AND target_schema = $2")
                        .as_str(),
                ))
                .bind(&module.module)
                .bind(&module.schema)
                .execute(&mut *conn)
                .await?;
            }
        }

        Ok(ResetReport {
            schemas,
            forgotten_migrations: forgotten,
        })
    }

    /// Modules recorded as installed, as `(module, schema, version)`.
    pub async fn installed_modules(
        &self,
        db: &PostgresDb,
    ) -> Result<Vec<(String, String, String)>, DbError> {
        self.validate()?;
        let mut conn = db.pool().acquire().await?;
        let table = self.qualified(&self.module_table);
        if !table_exists(&mut conn, &table).await? {
            return Ok(Vec::new());
        }

        let query = format!("SELECT module, target_schema, version FROM {table} ORDER BY 1, 2");
        let rows = sqlx::query_as(sqlx::AssertSqlSafe(query.as_str()))
            .fetch_all(&mut *conn)
            .await?;
        Ok(rows)
    }
}

async fn insert_ledger_row(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    table: &str,
    migration: &PlannedMigration,
) -> Result<(), DbError> {
    let query = format!(
        "INSERT INTO {table}
         (id, module, target_schema, migration_id, since, module_version, author, checksum)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         ON CONFLICT (id) DO NOTHING"
    );
    sqlx::query(sqlx::AssertSqlSafe(query.as_str()))
        .bind(&migration.id)
        .bind(&migration.module)
        .bind(&migration.schema)
        .bind(&migration.migration_id)
        .bind(migration.since.to_string())
        .bind(migration.module_version.to_string())
        .bind(&migration.author)
        .bind(&migration.checksum)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn table_exists(conn: &mut sqlx::PgConnection, table: &str) -> Result<bool, DbError> {
    let exists: (bool,) = sqlx::query_as(sqlx::AssertSqlSafe("SELECT to_regclass($1) IS NOT NULL"))
        .bind(table)
        .fetch_one(&mut *conn)
        .await?;
    Ok(exists.0)
}

/// Ledger table names come from configuration and are interpolated into DDL,
/// so they must be plain identifiers.
#[doc(hidden)]
pub fn validate_identifier(kind: &str, value: &str) -> Result<(), DbError> {
    let valid = !value.is_empty()
        && value.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !value.starts_with(|c: char| c.is_ascii_digit());

    if valid {
        Ok(())
    } else {
        Err(DbError::Unknown(format!(
            "invalid {kind} '{value}': expected an unquoted identifier"
        )))
    }
}

/// Stable 64-bit key for `pg_advisory_lock`.
#[doc(hidden)]
pub fn advisory_lock_key(name: &str) -> i64 {
    let digest = Sha256::digest(name.as_bytes());
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&digest[..8]);
    i64::from_be_bytes(bytes)
}
