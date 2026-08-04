# Quench Db

`quench-db` (crate `quench_db`) is a small ORM and database abstraction layer used across the Forge estate's Actix services. It provides a generic `Database` trait plus a `Model`/`Crud` pair for simple CRUD access over Postgres (via `sqlx`) or an in-memory backend, and — separately — a versioned migration catalog, planner and runner used to install and evolve database schemas. `quench-starter` re-exports `quench-db` as part of its `DbWrapper` bootstrap, and `foundry-service` is the dedicated job that drives the migration catalog against a real database.

## Public API / Key Types

- `Database` trait — `execute(&self, query: &str)`, `migrate(&self, migrations: Vec<Migration>)`.
- `Model` trait — `table_name() -> String`, `columns() -> Vec<&'static str>`, `primary_key_name() -> String` (defaults to `"id"`).
- `Crud<T: Model>` trait — `create`, `read`, `update`, `delete`, `list`, `find_by(column, value)`.
- `Db` enum — `Db::Postgres(PostgresDb)` / `Db::InMemory(InMemoryDb)`, built with `Db::connect(url)` (an empty URL yields an in-memory database); `Db::repository::<T>()` returns a `Repository<T>` that dispatches to the right backend.
- `PostgresDb` / `PostgresRepository<T>` — the Postgres-backed `Database` and `Crud` implementations, built on a `sqlx::Pool<Postgres>`. Pool size is controlled by `DB_POOL_MAX_SIZE` (default 5); the migration ledger table used by `Database::migrate` is named by `DB_MIGRATION_TABLE` (default `quench_migrations`).
- `InMemoryDb` / `InMemoryRepository<T>` — a `HashMap`-backed implementation for tests and local development.
- `DbError` — `ConnectionError`, `QueryError`, `SerializationError`, `NotFound`, `Unknown`, with `From<sqlx::Error>` and `From<serde_json::Error>` conversions.

### Migration catalog (`catalog`, `plan`, `runner`, `migrations`)

- `Catalog::load(root)` reads a directory of modules, each holding a `module.toml` (`ModuleManifest`: `name`, `version`, `scope` — `Schema` or `Database` — `default_schema`, `requires`, `variables`) plus `*.toml` migration files.
- `MigrationPlan::resolve(&catalog, &[InstallRequest::new("sage")])` walks the dependency graph, topologically orders the resolved `ResolvedModule`s, and renders each `PlannedMigration`'s SQL with `${var}` substitution.
- `MigrationRunner::new().apply(&db, &plan)` applies a plan to Postgres under a session advisory lock, recording results in a dedicated `foundry` schema (`forge_migrations`, `forge_modules` tables by default — configurable via `.ledger_schema()`, `.ledger_table()`, `.module_table()`). Supports `.dry_run(true)` and `.allow_drift(true)`. `MigrationRunner::reset(&db, &plan)` drops the schemas a plan owns and forgets the ledger rows (development only; never touches `public` or the ledger's own schema).
- `InstallRequest::parse("sage@0.1.9:sage_test")` parses `module[@version][:schema]` specs.

## Configuration

- `DB_POOL_MAX_SIZE` — Postgres connection pool size (default `5`).
- `DB_MIGRATION_TABLE` — table name used by the simple `Database::migrate` path (default `quench_migrations`); distinct from the catalog/runner's own ledger tables.

## Testing

`libs/quench-db/tests/unit.rs` wires unit tests under `tests/unit/` (`lib_tests.rs`, `runner_tests.rs`) covering `Db`/`checked_column` behavior and runner identifier validation. `libs/quench-db/tests/catalog_plan.rs` exercises catalog loading, requirement resolution and plan generation end-to-end.

## Usage example

```rust
use quench_db::{Crud, Model, PostgresDb, PostgresRepository};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
struct User {
    id: String,
    name: String,
}

impl Model for User {
    fn table_name() -> String {
        "users".to_string()
    }

    fn columns() -> Vec<&'static str> {
        vec!["id", "name"]
    }
}

async fn example() -> Result<(), quench_db::DbError> {
    let db = PostgresDb::new("postgres://user:pass@localhost/db").await?;
    let repo = PostgresRepository::<User>::new(db);

    let user = repo.read("1").await?;
    println!("{:?}", user);

    Ok(())
}
```

Migrations:

```rust
use quench_db::prelude::{Catalog, InstallRequest, MigrationPlan, MigrationRunner, PostgresDb};

async fn migrate(db: &PostgresDb) -> anyhow::Result<()> {
    let catalog = Catalog::load("migrations")?;
    let plan = MigrationPlan::resolve(&catalog, &[InstallRequest::new("sage")])?;
    let report = MigrationRunner::new().apply(db, &plan).await?;
    println!("{} migration(s) applied", report.results.len());
    Ok(())
}
```

[Home](../README.md)
