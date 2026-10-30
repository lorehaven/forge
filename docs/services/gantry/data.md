---
title: Data
parent: Gantry
grand_parent: Services
nav_order: 3
---

Gantry owns the Postgres schema `gantry` (`DB_SCHEMA`), created by the foundry module `gantry` (`services/foundry/service/migrations/gantry/`). It keeps nothing in Redis itself and no files except, for a local runner, the files of `GANTRY_STATE_DIR`. It uses raw SQL (no generic CRUD layer) because claiming the next queued operation needs a locked read followed by an update.

## Foundry module

`module.toml`: name `gantry`, version `0.1.0`, default schema `gantry`, requires `auth@^0.2` and `quench-core@^0.1`, variable `auth_schema = "auth"`. Migrations, in order (each `since = "0.1.0"`, author `gantry-service`):

| Id | Does |
|---|---|
| `0001-operations` | creates `operations` and its three indexes |
| `0002-overrides-plans` | creates `overrides` (later dropped) and `plans` with `plans_target_created_idx` |
| `0003-deployment-states` | creates `deployment_states` |
| `0004-inventory` | creates `inventory` |
| `0005-drop-overrides` | `DROP TABLE IF EXISTS overrides`: variables are no longer set by hand in Gantry; a package carries its own `values.yaml` and age-encrypted `secrets.yaml` |

## Tables

### operations

One row per requested change, written **before** anything is done.

| Column | Type | Notes |
|---|---|---|
| `id` | TEXT PK | UUID v4 |
| `kind` | TEXT NOT NULL | `install`, `upgrade`, `reinstall`, `downgrade`, `stop`, `start`, `swap`, `delete`, `apply`, `edit`, `refresh`, `custom` |
| `title` | TEXT NOT NULL | |
| `scope` | TEXT NOT NULL | default `'cluster'`; always `cluster` today |
| `state` | TEXT NOT NULL | default `'queued'`; `CHECK (state IN ('queued','running','succeeded','failed','rolled_back','cancelled'))` |
| `plan` | JSONB NOT NULL | default `'{}'`; the plan with its steps |
| `result` | JSONB | `{"runner": "<job name>"}` once finished |
| `runner_job` | TEXT | set at claim: `gantry-op-<12 characters of the id>` |
| `requested_by` | TEXT NOT NULL | the token's `sub`; **no foreign key** so a machine's client id is accepted |
| `cancel_requested` | BOOLEAN NOT NULL | default false |
| `error` | TEXT | |
| `log` | TEXT | the last 64 KiB of the runner output, written when it ends |
| `created_at` | TIMESTAMPTZ NOT NULL | default `NOW()` |
| `started_at`, `finished_at` | TIMESTAMPTZ | |

Indexes: unique partial `operations_one_running_per_scope_idx (scope) WHERE state = 'running'` (the database, not the service, refuses a second running operation per scope); `operations_state_created_idx (state, created_at)` (what the reconciler asks: oldest queued, every running); `operations_recent_idx (created_at DESC)` (history).

Claiming: `SELECT id FROM operations q WHERE q.state = 'queued' AND NOT EXISTS (SELECT 1 FROM operations r WHERE r.scope = q.scope AND r.state = 'running') ORDER BY q.created_at LIMIT 1`, then `UPDATE ... SET state = 'running', started_at = NOW(), runner_job = $2 WHERE id = $1 AND state = 'queued'`. Two services racing pick the same row; the unique index lets one `UPDATE` through and the other gets a unique violation, treated as "nothing to claim".

### plans

A plan is kept so that what is confirmed is exactly what was shown.

| Column | Type | Notes |
|---|---|---|
| `id` | TEXT PK | |
| `target` | TEXT NOT NULL | the package |
| `action` | TEXT NOT NULL | `install`, `upgrade`, `reinstall`, `downgrade`, `stop`, `start`, `swap` |
| `version` | TEXT | |
| `basis` | TEXT | what it was made against: the running version, or a deployment signature |
| `plan` | JSONB NOT NULL | |
| `created_by` | TEXT NOT NULL | |
| `created_at` | TIMESTAMPTZ NOT NULL | |
| `operation_id` | TEXT | NULL until confirmed; the sentinel `claiming` while a confirmation is in flight |

Index `plans_target_created_idx (target, created_at DESC)`. A plan is never deleted by the service.

### deployment_states

What each deployment is meant to be. A row exists only once someone has started or stopped it; otherwise the package's `default` applies.

| Column | Type | Notes |
|---|---|---|
| `deployment` | TEXT PK | |
| `desired` | TEXT NOT NULL | `CHECK (desired IN ('running','stopped'))` |
| `updated_by` | TEXT NOT NULL | the requester of the operation that changed it |
| `updated_at` | TIMESTAMPTZ NOT NULL | |

Written (upsert) when the operation that changes it **succeeds**, never when it is asked for.

### inventory

What each package declares, as the last successful install or refresh rendered it.

| Column | Type | Notes |
|---|---|---|
| `package` | TEXT PK | |
| `version` | TEXT NOT NULL | |
| `resources` | JSONB NOT NULL | array of `{apiVersion, kind, name, namespace}`; names and kinds only, never a manifest or a value |
| `updated_at` | TIMESTAMPTZ NOT NULL | |

## Retention

Nothing deletes operations or plans. The runner's own Job is deleted after its log is stored (the Job's `ttlSecondsAfterFinished` is a backstop of one day). Only the last 64 KiB of a runner's output is kept per operation.

## The in-memory store

`MemoryStore` implements the same `Store` interface for the unit tests. `main` always builds `PgStore`, which refuses an in-memory `Db` with `gantry needs Postgres; ...`.
