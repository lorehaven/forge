# Example: Db

`db_example` (`examples/db_example`, package `quench-db-example`) demonstrates `quench-db`'s migration runner and CRUD repository against a real Postgres database. It exists as the workspace's runnable reference for how a service is expected to define a model, load Liquibase-style migrations from a directory, and perform create/list/update/delete through `PostgresRepository`.

## What it covers

- **A `Model`** (`Task { id, title, completed, priority }`, deriving `Serialize`, `Deserialize`, `FromRow`, `Clone`) implementing `quench_db::prelude::Model` with `table_name()` (`"tasks"`) and `columns()`.
- **Migration loading**: `MigrationLoader::load_from_dir("examples/db_example/migrations")` reads every `.toml` file under `migrations/`, run in order via `db.migrate(migrations)`:
  - `00001-initial-schema.toml` — `createTable` for `tasks` (`id TEXT PRIMARY KEY`, `title TEXT NOT NULL`, `completed BOOLEAN NOT NULL`).
  - `00002-add-priority.toml` — a raw `sql` change: `ALTER TABLE tasks ADD COLUMN IF NOT EXISTS priority INT DEFAULT 0`.

  Each file's `[[migrations]]` entry has its own `id` and `author`, and can carry one or more `[[migrations.changes]]` entries — either structured (`createTable`) or raw `sql`.
- **CRUD via `PostgresRepository<Task>`**: create a task, list all tasks, flip `completed` and update, then delete it — printing each step's result to stdout.
- Graceful handling when Postgres isn't reachable: `PostgresDb::new(&db_url)` failure prints a warning and exits `Ok(())` rather than panicking.

## How to run it

```bash
export DATABASE_URL=postgres://postgres:postgres@localhost:5432/postgres  # or your own
cargo run -p quench-db-example
```

If `DATABASE_URL` isn't set, it defaults to `postgres://postgres:postgres@localhost:5432/postgres` (see `src/main.rs`). The binary is named `db_example` (`[[bin]] name = "db_example"` in `Cargo.toml`), but since `cargo run -p` selects by package name, `quench-db-example` is what you pass to `-p`.

Because the migration path (`examples/db_example/migrations`) is relative, run the binary from the workspace root — `cargo run -p quench-db-example` from anywhere inside the workspace does this correctly since Cargo sets the working directory to the workspace root for `cargo run`.

## Requirements

- A reachable Postgres instance at `DATABASE_URL` (the example degrades gracefully and exits cleanly if none is found).
- Dependencies: `quench-db`, `tokio`, `serde`, `sqlx`, `anyhow`, `tracing-subscriber` (all workspace-managed). `Cargo.toml` sets `publish = false`.

[Home](../README.md)
