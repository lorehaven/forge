use quench_db::prelude::{Crud, Database, MigrationLoader, Model, PostgresDb, PostgresRepository};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::env;

// 1. Define your Model
#[derive(Debug, Serialize, Deserialize, FromRow, Clone)]
struct Task {
    id: String,
    title: String,
    completed: bool,
    priority: i32,
}

impl Model for Task {
    fn table_name() -> String {
        "tasks".to_string()
    }

    fn columns() -> Vec<&'static str> {
        vec!["id", "title", "completed", "priority"]
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Get database URL
    let db_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/postgres".to_string());

    println!("Connecting to database: {}", db_url);
    let db = match PostgresDb::new(&db_url).await {
        Ok(db) => db,
        Err(e) => {
            eprintln!(
                "Failed to connect to database: {}. Ensure Postgres is running.",
                e
            );
            return Ok(());
        }
    };

    // 2. Load and Run Migrations from Directory (Liquibase-style)
    println!("Loading migrations from migrations/ directory...");
    let migrations = MigrationLoader::load_from_dir("examples/db_example/migrations")?;

    println!("Running {} migrations...", migrations.len());
    db.migrate(migrations).await?;
    println!("Migrations completed successfully.");

    // 3. CRUD Operations
    let repo = PostgresRepository::<Task>::new(db.clone());

    // --- Cleanup from previous runs ---
    repo.delete("task-1").await.ok();

    // Create
    println!("\n--- Creating Task ---");
    let task = Task {
        id: "task-1".into(),
        title: "Manage Multi-File Migrations".into(),
        completed: false,
        priority: 10,
    };
    let created = repo.create(&task).await?;
    println!("Created: {:?}", created);

    // List
    println!("\n--- Listing All Tasks ---");
    let all_tasks = repo.list().await?;
    for t in all_tasks {
        println!(
            " - [{}]: {} (completed: {}, priority: {})",
            t.id, t.title, t.completed, t.priority
        );
    }

    // Update
    println!("\n--- Updating Task ---");
    let mut to_update = created;
    to_update.completed = true;
    let updated = repo.update(&to_update).await?;
    println!("Updated: {:?}", updated);

    // Delete
    println!("\n--- Deleting Task ---");
    repo.delete("task-1").await?;
    println!("Deleted task-1");

    Ok(())
}
