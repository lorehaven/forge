---
title: Foundry
parent: Services
nav_order: 4
has_children: true
---

Foundry installs the Forge database schemas. It has no HTTP server and no UI: `foundry-service` is a run-to-completion command that reads a catalog of versioned migration modules, resolves the modules you ask for and their dependencies, applies the outstanding migrations in dependency order and exits. It runs as a Kubernetes Job (or init container) before a rollout, as a `foreman` task before the services start, and as a step of the coverage pipeline.

* Crate: `foundry` 0.2.35 (`services/foundry/service`), binary `foundry-service` (clap name `foundry`), library `foundry_service` (the `config` module only).
* The engine is not in this repository's service: `Catalog`, `MigrationPlan` and `MigrationRunner` come from `quench-db` (see the Quench book).
* Dependencies of the crate: `quench-args`, `quench-cli`, `quench-core`, `quench-db`, `quench-log`, `quench-serde`, `tokio`; dev: `quench-test`.
* Needs Postgres for every command except `validate`. Needs no other service.
* Authentication, ports, `BASE_PATH`, health endpoints: none. There is no `openapi.yaml` content to speak of; the file exists with an empty `paths` object so the checker (`scripts/docs-check.py foundry`) can confirm there are no routes.

## Pages

| Page | Content |
|---|---|
| [CLI](cli.md) | Every command and flag, output lines, exit behaviour. |
| [Catalog](catalog.md) | `module.toml`, migration files, change kinds, `${var}` rendering, versions, scopes, dependency resolution, checksums. |
| [Configuration](configuration.md) | Flags, environment variables, `config/install.toml`, precedence. |
| [Data](data.md) | The ledger tables foundry owns and every table the catalog creates for the services documented in this part of the book. |
| [Operations](operations.md) | foreman, image, Job, pipeline, failure modes. |
| [API](api.md) | There is none; states so. |
| [Client](client.md) | There is none; states so. |

## What is in the catalog

`services/foundry/service/migrations/` holds 13 modules (checked with `foundry-service validate`, "13 module(s)"):

| Module | Version | Scope | Default schema | Requires |
|---|---|---|---|---|
| `quench-core` | 0.1.0 | schema | `quench-core` (module name) | none |
| `auth` | 0.2.3 | database | `auth` | none |
| `pgvector` | 0.1.0 | database | `public` | none |
| `gatehouse` | 0.1.0 | schema | `auth` | `auth@^0.2` |
| `sage` | 0.2.0 | schema | `sage` | `auth@^0.2`, `quench-core@^0.1`, `pgvector@^0.1` |
| `switchboard` | 0.2.0 | schema | `switchboard` | `auth@^0.2`, `quench-core@^0.1` |
| `workbench` | 0.1.0 | schema | `workbench` | `auth@^0.2`, `quench-core@^0.1` |
| `gauge` | 0.4.0 | schema | `gauge` | `quench-core@^0.1` |
| `warehouse`, `conveyor`, `daybook`, `gantry`, `hoist` | see their own pages | | | |

`gatehouse` has no migration files: it exists so that `gatehouse` is a nameable install target whose state is the `auth` module.

Note: `config/install.toml` does not list `gauge` (see [Configuration](configuration.md)).
