---
title: CLI
parent: Foundry
grand_parent: Services
nav_order: 7
---

`foundry-service [GLOBAL OPTIONS] [COMMAND]`. Without a command, `apply` runs. Source: `services/foundry/service/src/main.rs`; the help text below is the output of `foundry-service --help`.

```text
Install versioned database modules from the Forge migration catalog

Usage: foundry [OPTIONS] [COMMAND]

Commands:
  apply     Resolve the plan and apply every outstanding migration
  plan      Show what `apply` would do, without writing anything
  status    Show installed module versions against the catalog
  validate  Check the catalog and resolve the plan without touching the database
  reset     Drop everything the plan owns and reinstall it from scratch
  help      Print this message or the help of the given subcommand(s)
```

## Global options

All options are `global`: they may come before or after the command.

| Option | Value | Default | Meaning |
|---|---|---|---|
| `-c`, `--config <FILE>` | path | `$FOUNDRY_CONFIG`, else `config/install.toml` if it exists | Install configuration file. A path given explicitly (flag or env) that does not exist is an error: `config <path> does not exist`. |
| `--catalog <DIR>` | path | `$FOUNDRY_CATALOG`, else the file's `catalog`, else `migrations` | Catalog directory. |
| `--database-url <URL>` | string | `$DATABASE_URL`, else `$POSTGRES_URL`, else the file's `database_url` | Postgres connection string. |
| `-i`, `--install <SPEC>` | `module[@version][:schema]`, repeatable | `$FOUNDRY_INSTALL`, else the file's `[[install]]` | Replaces the file's install list entirely. |
| `--ledger-schema <SCHEMA>` | identifier | `$FOUNDRY_LEDGER_SCHEMA`, file, `foundry` | Schema holding the ledger tables. |
| `--ledger-table <TABLE>` | identifier | `$FOUNDRY_LEDGER_TABLE`, file, `forge_migrations` | Applied-migration ledger. |
| `--module-table <TABLE>` | identifier | `$FOUNDRY_MODULE_TABLE`, file, `forge_modules` | Installed-module ledger. |
| `--allow-drift` | flag | off | A recorded migration whose SQL changed is a warning instead of an error. |
| `--yes` | flag | off | Confirms `reset`. |
| `-h`, `--help`, `-V`, `--version` | | | Standard. |

The three ledger identifiers must be plain unquoted identifiers (`[A-Za-z0-9_]`, not starting with a digit), otherwise the run fails with `invalid <kind> '<value>': expected an unquoted identifier`.

An install spec `module@version:schema`: `@version` is a full semver (`Version::parse`), `:schema` an optional target schema; an empty schema after `:` is ignored. Empty module name is an error (`missing module name`). Examples: `sage`, `sage@0.2.0`, `switchboard:switchboard_staging`, `sage@0.1.9:sage_test`.

## Start-up sequence (every command)

1. Initialise logging (`quench_log::init`).
2. Parse arguments; resolve the configuration (see [Configuration](configuration.md)). Failures: `no database configured: set DATABASE_URL or pass --database-url` (not raised for `validate`), `nothing to install: declare [[install]] entries in config/install.toml, set FOUNDRY_INSTALL, or pass --install`.
3. Load the catalog (`failed to load catalog <dir>`); print `catalog  N module(s) from <dir>`.
4. Resolve the plan (unknown module, unsatisfiable version requirement, cycle, bad install spec, unknown template variable are errors); print one `module` line per module instance in apply order, with `(required by X)` for dependencies, and a `plan  N migration(s) across M module instance(s)` line.
5. Run the command.

Output lines are `print_status(tone, label, text)` lines (coloured label, then text).

## Commands

### `apply` (default)

Connects (`db  connected to Postgres`; failure: `failed to connect to the database`), takes the advisory lock, creates the ledger schema and tables if missing, then for each planned migration in order: skips it if its id is in the ledger (checking the checksum), otherwise runs it in its own transaction and records it. Finally records every module instance in the module table (upsert). Output: one line per migration, label `applied` or `current`; then `done  N applied, M already up to date`. Exit 0. Any error: non-zero (the `Result` returned from `main`).

### `plan`

Same resolution, then a dry run against the database: connects, reads the ledger (does not create it, takes no lock), every unapplied migration is labelled `pending`, applied ones `current`, then `dry-run  N migration(s) would be applied`. Needs a database. A checksum mismatch is still an error (or a warning with `--allow-drift`).

### `status`

Connects and lists the module table: `installed  <module> <version> in schema <schema> (catalog: <catalog version | not in catalog>)`. Success tone when the versions are equal, warning tone otherwise. With no module table or no rows: `status  no modules installed yet`. Ignores the plan (it still has to resolve, so `--install`/config must be valid).

### `validate`

Loads the catalog and resolves the plan; no database is opened and `DATABASE_URL` is not required. Prints `validate  catalog and plan are consistent`. Used by the tests against the real catalog.

### `reset`

Development only. Without `--yes`: prints `reset  refusing to drop schemas without --yes; this destroys data`, one `would drop  schema <s> (CASCADE)` line per resettable schema, and exits with status 1 (before connecting). With `--yes`: connects, takes the lock, runs `DROP SCHEMA IF EXISTS <s> CASCADE` for each schema of the plan's schema-scoped module instances, except `public` and the ledger schema; deletes the ledger rows of the plan's migrations and the module rows of the plan's module instances (when those tables exist); prints `dropped  schema <s>` per schema and `reset  forgot N recorded migration(s)`; then applies the plan like `apply` and prints `reset  N migration(s) reinstalled`.

Consequence worth knowing: database-scoped modules (`auth`, `pgvector`) have no schema of their own in the drop list, so `auth` is not dropped by `reset`; but its ledger rows are forgotten and the migrations are re-applied (`IF NOT EXISTS` statements make that harmless). A `quench-core` instance installed into `sage` is dropped with the `sage` schema.

## Exit status

0 on success. 1 for `reset` without `--yes`. Any other error is returned from `main` and the process exits non-zero with `Error: <message>` on stderr (seen: `Error: no database configured: ...`).

## Examples

```bash
# What would be installed, no database needed
foundry-service --catalog services/foundry/service/migrations --install sage validate

# Pipeline form (.conveyor.toml)
foundry-service apply --catalog services/foundry/service/migrations \
  --database-url postgres://postgres:postgres@127.0.0.1:5432/postgres \
  --install conveyor --install daybook --install gantry --install gatehouse \
  --install sage --install switchboard --install warehouse --install workbench

# Per-environment override without rebuilding the image
FOUNDRY_INSTALL="sage@0.2.0,switchboard:switchboard_staging" foundry-service apply
```

Sample `validate` output for `--install 'sage@0.2.0:sage_test'` (colours removed):

```text
catalog 13 module(s) from migrations
module 1. auth 0.2.3 -> schema auth (required by sage)
module 2. pgvector 0.1.0 -> schema public (required by sage)
module 3. quench-core 0.1.0 -> schema sage_test (required by sage)
module 4. sage 0.2.0 -> schema sage_test
plan 16 migration(s) across 4 module instance(s)
validate catalog and plan are consistent
```

The `quench-core` line was reconstructed from the numbering of the captured run (it was filtered from the capture); its schema is the requiring module's schema.
