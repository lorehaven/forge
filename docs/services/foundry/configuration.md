---
title: Configuration
parent: Foundry
grand_parent: Services
nav_order: 2
---

Source: `services/foundry/service/src/config.rs` (`RunConfig::resolve`). Precedence for every setting: command-line flag, then environment variable, then config file, then default. Empty environment variables count as unset (`env_value` filters empty strings).

## Environment variables

| Variable | Flag | Default | Meaning |
|---|---|---|---|
| `DATABASE_URL` | `--database-url` | none | Postgres URL. Checked before `POSTGRES_URL`. |
| `POSTGRES_URL` | `--database-url` | none | Fallback for `DATABASE_URL`. |
| `FOUNDRY_CATALOG` | `--catalog` | `migrations` | Catalog directory. |
| `FOUNDRY_CONFIG` | `--config` | `config/install.toml` if present | Config file. If set and missing: `config <path> does not exist`. |
| `FOUNDRY_INSTALL` | `--install` | the file's `[[install]]` | Comma separated specs `module[@version][:schema]`, trimmed, empty items dropped. Ignored when any `--install` flag is given. |
| `FOUNDRY_LEDGER_SCHEMA` | `--ledger-schema` | `foundry` | |
| `FOUNDRY_LEDGER_TABLE` | `--ledger-table` | `forge_migrations` | |
| `FOUNDRY_MODULE_TABLE` | `--module-table` | `forge_modules` | |
| `FOUNDRY_TEST_DATABASE_URL` | | none | Tests only: `cli_tests` run `apply`/`status`/`plan`/`reset` checks against it; they skip when unset. |
| (logging) | | | Configured from the environment by `quench_log::init()` (`Config::from_env`); see `quench-log` in the Quench book. |

## Config file (`config/install.toml`)

TOML, unknown keys rejected (`deny_unknown_fields`).

```toml
catalog = "migrations"          # optional
database_url = "postgres://..." # optional (avoid: use the environment)
ledger_schema = "foundry"       # optional
ledger_table = "forge_migrations"
module_table = "forge_modules"

[[install]]
module = "sage"
version = "0.2.0"               # optional semver
schema = "sage"                 # optional
[install.variables]             # optional, overrides module [variables]
embedding_dim = "1024"
```

Each `[[install]]` has `module` (required), `version`, `schema`, `variables`. Specs from `--install` or `FOUNDRY_INSTALL` replace the entire list; they cannot carry variables.

The shipped file installs `conveyor`, `daybook`, `gantry`, `gatehouse`, `hoist`, `sage`, `switchboard`, `warehouse`, `workbench`. `gauge` is not listed, so an unmodified Job does not create the `gauge` schema (gauge runs without a database but loses history; see the gauge pages). `foreman` passes `--install <service>` for each selected service, and CI passes explicit installs.

## Build and image

The image is built from `images/Dockerfile.alpine` with `RESOURCE_DIR=migrations` and `RUN_AS=999:999` (`.anvil/docker.toml`, module `foundry-service`, image `foundry`).
