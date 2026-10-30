---
title: Catalog
parent: Foundry
grand_parent: Services
nav_order: 6
---

The catalog is a directory (default `migrations`, in the repository `services/foundry/service/migrations`) with one subdirectory per module. Loaded by `quench_db::catalog::Catalog::load`. Source of truth: `quench-db/src/catalog.rs`, `migrations.rs`, `plan.rs`, `runner.rs` (Quench repository).

## Layout

```text
migrations/
  sage/
    module.toml
    0001-workspace.toml
    0002-files-and-rag.toml
```

Migration files are the `*.toml` files of the module directory other than `module.toml`, sorted by file name. Inside a file, migrations are sorted by `id`. The first `-`-separated token of an id (the `0001` prefix) must be unique inside one file, otherwise loading fails with `Duplicate migration prefix`. Ids are not required to be unique across files by the loader.

## `module.toml`

| Key | Type | Default | Meaning |
|---|---|---|---|
| `name` | string | required | Module name (install target). |
| `version` | semver | required | Current version of the module; installing without a version uses it. |
| `description` | string | none | Free text. |
| `scope` | `"schema"` or `"database"` | `schema` | `database`: one instance per database whatever schema asks for it. |
| `default_schema` | string | module name for `schema` scope, `public` for `database` | Schema used when the install request names none. |
| `requires` | list | `[]` | Dependencies; each is `"module@req"` (req default `*`) or a table `{ module, version, schema }`. |
| `[variables]` | table of strings | empty | Default `${var}` values for the module's SQL. |

Version requirements use `VersionReq` (`^0.2`, `*`, ...). Real manifests: `sage` requires `auth@^0.2`, `quench-core@^0.1`, `pgvector@^0.1` and defines `embedding_dim = "1024"` and `auth_schema = "auth"`.

## Migration files

```toml
[[migrations]]
id = "0002-files-and-rag"
author = "sage-service"
since = "0.2.0"

[[migrations.changes]]
sql = """
CREATE TABLE IF NOT EXISTS ${schema}.files (...);
"""
```

| Key | Meaning |
|---|---|
| `id` | `NNNN-name`. |
| `author` | Recorded in the ledger. |
| `since` | Module version that introduced the migration; default `0.0.0` (always applied). A migration is planned only when `since <= version being installed`. An unparsable `since` counts as `0.0.0`. |
| `changes` | List of change sets, each rendered to one SQL string. |

Change kinds (enum `ChangeSet`):

| Key | Rendered SQL |
|---|---|
| `sql = "..."` | The string as written (may hold several statements; run with the simple protocol). |
| `createSchema = { name }` | `CREATE SCHEMA IF NOT EXISTS <name>` |
| `createExtension = { name }` | `CREATE EXTENSION IF NOT EXISTS <name>` |
| `createTable = { schema?, name, columns = [{ name, data_type, primary_key?, nullable? }] }` | `CREATE TABLE IF NOT EXISTS <schema.>name (col type [PRIMARY KEY] [NOT NULL], ...)`; `NOT NULL` only when `nullable = false`. |

## Variables

`${name}` placeholders are substituted in every rendered statement. Unknown names and an unterminated `${` are errors (`unknown variable '<n>' (known: ...)`). Precedence: the module's `[variables]`, overridden by the install request's `variables` (only possible from a `[[install]]` entry in the config file), overridden by the built-ins `schema` (target schema), `module` and `version` (the version being installed).

## Resolution and order

1. Each install request names a module (error `unknown module` otherwise), an optional version (default the catalog version) and an optional schema (default `default_schema`).
2. `requires` are followed recursively. A dependency's schema: the one named in the requirement, otherwise the requiring module's schema; a `scope = "database"` dependency has one instance whatever the schema (instance key `module@*`).
3. After all instances and versions are known, each requirement's version range is validated against the chosen versions.
4. The graph is topologically sorted (dependencies first); the plan lists migrations module by module in that order, each module's migrations in catalog order.
5. Instance identity in the ledger: `module@schema` (`module@*` for database scope). A ledger row id is `module@schema:migration-id` (for database scope `module@*:migration-id`).

Example: `--install sage` in the real catalog yields `auth`, `pgvector`, `quench-core -> schema sage`, `sage`. Installing `workbench` and `sage` together gives one `quench-core` instance per schema (`workbench`, `sage`) and a single `auth` and `pgvector`.

## Checksums and drift

The checksum of a planned migration is SHA-256 over every rendered statement followed by `;\n`. `since` is not part of it. If the ledger already holds the id with a different checksum, `apply` and `plan` fail with `migration '<id>' changed after it was applied (recorded <old>, catalog <new>)`; with `--allow-drift` this is logged as a warning and the migration counts as already applied.

## Execution

Each migration runs in one transaction: all its statements, then the ledger insert (`ON CONFLICT (id) DO NOTHING`), then commit. A failing statement aborts the migration with `migration '<id>': <error>`; earlier migrations stay applied. A session-level advisory lock `pg_advisory_lock(key)` with `key` = first 8 bytes (big endian) of SHA-256 of the qualified ledger table name serialises concurrent `apply` and `reset` runs; it is released explicitly after the run, also on error. `plan` (dry run) takes no lock and creates nothing.

## Adding a migration

1. Add a file `NNNN-name.toml` to the module directory (idempotent SQL: `IF NOT EXISTS`).
2. Tag `since` with the version that will ship it.
3. Bump `version` in `module.toml` and in the service crate.
4. `foundry-service validate`, then `plan`.

Never edit an applied migration: the checksum drift check will stop the next run.
