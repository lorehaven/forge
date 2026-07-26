# Quench DB

A simple ORM and database abstraction layer for the Forge project.

## Features

- Generic `Database` and `Model` traits.
- PostgreSQL support via `sqlx`.
- Generic `Crud` interface for basic operations.
- Versioned migration catalog with dependency resolution (`catalog`, `plan`,
  `runner`).

## Usage

```rust
use quench_db::{PostgresDb, PostgresRepository, Model, Crud};
use serde::{Serialize, Deserialize};

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
struct User {
    id: String,
    name: String,
}

impl Model for User {
    fn table_name() -> &'static str {
        "users"
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

## Migration catalog

Migrations live in a catalog of independently versioned modules that declare
what they require. Resolution produces an ordered, rendered plan; the runner
applies it and records the result.

```rust
use quench_db::prelude::{Catalog, InstallRequest, MigrationPlan, MigrationRunner, PostgresDb};

async fn migrate(db: &PostgresDb) -> anyhow::Result<()> {
    let catalog = Catalog::load("migrations")?;
    let plan = MigrationPlan::resolve(&catalog, &[InstallRequest::new("sage")])?;

    // Dependencies first, each module instanced per target schema.
    let report = MigrationRunner::new().apply(db, &plan).await?;
    println!("{} migration(s) applied", report.results.len());
    Ok(())
}
```

Module manifests (`module.toml`) carry `version`, `requires`, `scope`
(`schema` or `database`), a `default_schema` and `${var}` defaults; migrations
carry `since` so a requested version selects exactly which of them apply.

`foundry-service` is the ready-made job that drives this against a real
database - see its README for the catalog layout and operational details.
