# Quench DB

A simple ORM and database abstraction layer for the Forge project.

## Features

- Generic `Database` and `Model` traits.
- PostgreSQL support via `sqlx`.
- Generic `Crud` interface for basic operations.

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
