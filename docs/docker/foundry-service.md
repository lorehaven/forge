# Foundry Service

Foundry is the Forge estate's database initialization job: one place for every service's migrations, one command to install a chosen set of them at chosen versions. It is not a long-running server — it is a run-to-completion CLI, meant to run as a Kubernetes `Job` (or an init container) ahead of a rollout, or as a `foreman` task ahead of starting services locally. It exits `0` when the database matches the declared plan and non-zero otherwise. Binary and crate name: `foundry-service` (`docker/foundry-service`).

## Features

- **A migration catalog**, one independently versioned module per directory under `migrations/`.
- **Dependency resolution and topological ordering** — declare *what* you want installed, foundry works out everything it needs and the order to apply it in.
- **Per-schema module instances** — one module definition can be installed into several schemas.
- **A durable ledger** (`foundry.forge_migrations`, `foundry.forge_modules`) with checksum drift detection.
- **`plan`/`status`/`validate`** for previewing and auditing without writing, and a development-only `reset`.

## The migration-module system

`migrations/<module>/` holds one independently versioned module, described by a `module.toml`:

```toml
name = "sage"
version = "0.2.0"                          # tracks the service crate version
default_schema = "sage"
requires = ["auth@^0.1", "quench-core@^0.1", "pgvector@^0.1"]

[variables]
embedding_dim = "1024"                     # substituted as ${embedding_dim}
```

and numbered migration files tagged with the module version that introduced them:

```toml
[[migrations]]
id = "0002-files-and-rag"
author = "sage-service"
since = "0.2.0"

[[migrations.changes]]
sql = "CREATE TABLE IF NOT EXISTS ${schema}.files (...);"
```

The catalog today holds `quench-core`, `auth`, `pgvector`, `gatehouse` (an empty module whose whole state is `auth` — it exists only so `gatehouse` is a nameable install target), `sage`, `switchboard`, `warehouse` and `conveyor` — one directory per service that owns database state, plus the shared/base modules they all depend on.

### Dependency resolution and apply order

An install request names a module and (optionally) a version and schema, e.g. `sage@0.2.0`. Foundry resolves `requires` recursively — `sage` pulls in `auth`, `quench-core` and `pgvector` — topologically sorts the resulting graph, and applies every migration with `since <=` the version being installed, in dependency order. Installing `sage@0.2.0` applies every `sage` migration tagged `0.2.0` or earlier; pinning `sage@0.1.0` stops before the `0.2.0` ones, and upgrading later applies only the gap. `Catalog::load` + `MigrationPlan::resolve` (`libs/quench-db`) do this work; `foundry-service`'s `main.rs` is a thin CLI wrapper around it.

### Version pinning and drift

Each migration's checksum is computed **only from its rendered SQL** — the `since` tag is deliberately excluded, so retagging a migration during a version renumber does not cause drift on a database that already applied it under the old tag. If a migration's SQL changes after it was applied, the run fails with a drift error (`--allow-drift` downgrades that to a warning). A session-level Postgres advisory lock (`pg_advisory_lock`, keyed off the ledger table name) serializes concurrent runs, so a `Job` racing an init container — or two developers running `foreman` at once — is safe.

### Modules are installed per schema

A module instance is `module@schema`; one module definition can serve several schemas, since `${schema}` is rendered per instance. A dependency inherits the schema of whatever required it unless the requirement names one explicitly (`{ module = "quench-core", version = "^0.1", schema = "shared" }`). Modules with `scope = "database"` (`auth`, `pgvector`) resolve to exactly one instance per database no matter how many schemas depend on them — the shared identity realm and the `vector` extension are each one instance, not one per service.

### Bookkeeping

Lives in its own `foundry` schema, so it is never dropped alongside the schemas it tracks: `foundry.forge_migrations` (applied migrations, with a checksum of their rendered SQL) and `foundry.forge_modules` (installed module versions). `--ledger-schema` moves it elsewhere if needed.

## Usage

```bash
foundry-service apply       # resolve and apply (default)
foundry-service plan        # what apply would do; writes nothing
foundry-service status      # installed module versions vs. the catalog
foundry-service validate    # check catalog + plan without a database
foundry-service reset --yes # drop the planned schemas and their ledger rows
```

`reset` is a development convenience: it refuses to run without `--yes`, and never drops `public` or the ledger schema itself.

Configuration precedence is flags, then environment, then `config/install.toml`:

| Flag | Environment | Default |
| --- | --- | --- |
| `--database-url` | `DATABASE_URL`, `POSTGRES_URL` | required |
| `--install module[@version][:schema]` | `FOUNDRY_INSTALL` (comma-separated) | `[[install]]` entries |
| `--catalog` | `FOUNDRY_CATALOG` | `migrations` |
| `--config` | `FOUNDRY_CONFIG` | `config/install.toml` |
| `--ledger-schema` | `FOUNDRY_LEDGER_SCHEMA` | `foundry` |
| `--ledger-table` | `FOUNDRY_LEDGER_TABLE` | `forge_migrations` |
| `--module-table` | `FOUNDRY_MODULE_TABLE` | `forge_modules` |

## Requirements

- Postgres reachable at `DATABASE_URL`/`POSTGRES_URL` for every command except `validate`, which only reads the catalog.
- No other service dependency — foundry runs ahead of everything else it installs schemas for.

## Configuration

`config/install.toml` is the baked-in default install list (currently `conveyor`, `gatehouse`, `sage`, `switchboard`, `warehouse` — shared modules like `auth`, `quench-core` and `pgvector` resolve automatically and never need listing). Override per environment without rebuilding the image via `FOUNDRY_INSTALL`, e.g. `FOUNDRY_INSTALL="sage@0.2.0,switchboard:switchboard_staging"`.

As a Kubernetes `Job`, the image needs no command — `apply` runs when given none. It runs as uid 999 (owner of everything under `/app`), so no `securityContext` is needed to keep it off root; deployment manifests themselves live outside this repository.

In local dev, `foreman` runs foundry as a `[tasks.foundry]` migrate step ahead of starting services — `each_selected = ["--install", "${service}"]` turns "start this subset of the estate" directly into the matching `--install` flags, so a partial `foreman start conveyor` only installs conveyor's (and its dependencies') schemas. `foreman reset` runs foundry's `reset --yes` the same way.

## Adding a migration

1. Add `NNNN-name.toml` to the module directory.
2. Tag it with the `since` version that will ship it.
3. Bump `version` in `module.toml` **and the service crate** to match — under Cargo's 0.x rules the minor slot is the breaking one, so a migration existing deployments must coordinate around is `0.1.x → 0.2.0`, not a patch bump.
4. `foundry-service validate` to check the catalog, `plan` to preview.

## Testing

No dedicated `tests/` directory in `docker/foundry-service` itself — the migration-resolution logic it wraps (`Catalog`, `MigrationPlan`, `MigrationRunner`) lives in and is tested by `libs/quench-db` (see `libs/quench-db/tests/catalog_plan.rs`). `foundry-service validate` and `plan` double as a manual smoke test against the real catalog.

[Home](../README.md)
