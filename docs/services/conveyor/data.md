---
title: Data
parent: Conveyor
grand_parent: Services
nav_order: 3
---

Conveyor owns the Postgres schema `conveyor` (`DB_SCHEMA`). The tables are created by the foundry module `conveyor` (`services/foundry/service/migrations/conveyor/`), never by the service. It keeps nothing in Redis itself (Redis holds realm sessions) and on disk only the per-run checkouts under `CONVEYOR_WORK_DIR`.

## Foundry module

`module.toml`: name `conveyor`, version `0.1.0`, description `Conveyor - CI/CD repositories, runs, jobs, logs and artifacts`, default schema `conveyor`, requires `auth@^0.2` and `quench-core@^0.1`, variable `auth_schema = "auth"` (the realm schema that owns `users`). Migrations apply in order; each is `since = "0.1.0"`, author `conveyor-service`.

| Id | Adds |
|---|---|
| `0001-repos-and-runs` | `repos`, `runs`, `jobs`, `steps`, `logs`, `artifacts` and their indexes |
| `0002-secrets` | `secrets` |
| `0003-run-concurrency` | the one-running-run-per-repository index; `runs.cancel_requested` |
| `0004-projects` | `projects`; `repos.project_id` (nullable, backfilled, then `NOT NULL`) |
| `0005-credentials` | `credentials` |
| `0006-restart` | `runs.resumed_from`, `jobs.reused_from_run` |
| `0007-subscriptions` | `subscriptions`, `notification_outbox` |
| `0008-schedules` | `schedules` |
| `0009-run-stages` | `runs.stages` |

`0004` backfills a tree for existing data: one root project per distinct `repos.owner`, one child per repository named after it, then points each repository at its child.

## Tables

`created_at` / `updated_at` / `queued_at` defaults are `NOW()`. `TIMESTAMPTZ` everywhere. No `CHECK` constrains the status or trigger text; the code parses them and a row it cannot parse is reported as `unreadable row: unknown status '<x>'` (`500`).

### repos

A repository conveyor is willing to build.

| Column | Type | Notes |
|---|---|---|
| `id` | TEXT PK | UUID v4 |
| `provider` | TEXT NOT NULL | `github` or `generic` |
| `owner`, `name` | TEXT NOT NULL | the provider's identity of the repository (what a webhook names) |
| `clone_url` | TEXT NOT NULL | |
| `default_branch` | TEXT NOT NULL | default `'master'` |
| `registered_by` | TEXT NOT NULL | FK `auth.users(username)` ON DELETE CASCADE |
| `enabled` | BOOLEAN NOT NULL | default true |
| `project_id` | TEXT NOT NULL | FK `projects(id)` (no cascade: a project with a repository cannot be deleted) |
| `created_at`, `updated_at` | TIMESTAMPTZ | |

Unique index `repos_provider_slug_idx (provider, owner, name)`.

### runs

A run is also its own queue entry.

| Column | Type | Notes |
|---|---|---|
| `id` | TEXT PK | |
| `repo_id` | TEXT NOT NULL | FK `repos(id)` ON DELETE CASCADE |
| `trigger` | TEXT NOT NULL | `push`, `pull_request`, `manual`, `schedule` |
| `git_ref`, `sha` | TEXT NOT NULL | full ref; the commit built |
| `message` | TEXT | head commit subject / pull request title |
| `delivery_id` | TEXT | provider delivery id, or `schedule:<id>:<unix>` |
| `status` | TEXT NOT NULL | default `'queued'` |
| `queued_at`, `started_at`, `finished_at` | TIMESTAMPTZ | `started_at` is the first claim |
| `claimed_by`, `claimed_at` | TEXT, TIMESTAMPTZ | the worker and its last heartbeat |
| `attempt` | INTEGER NOT NULL | default 0; +1 per claim |
| `error` | TEXT | why it ended when the jobs do not say |
| `cancel_requested` | BOOLEAN NOT NULL | default false (added by `0003`) |
| `resumed_from` | TEXT | FK `runs(id)` ON DELETE SET NULL |
| `stages` | TEXT | JSON array of stage names, or NULL for the whole pipeline |

Indexes: unique `runs_delivery_id_idx (delivery_id) WHERE delivery_id IS NOT NULL` (what makes a redelivery a no-op); `runs_queue_idx (status, queued_at)`; `runs_repo_recent_idx (repo_id, queued_at DESC)`; unique `runs_one_running_per_repo_idx (repo_id) WHERE status = 'running'`.

### jobs

| Column | Type | Notes |
|---|---|---|
| `id` | TEXT PK | |
| `run_id` | TEXT NOT NULL | FK `runs(id)` ON DELETE CASCADE |
| `stage`, `name` | TEXT NOT NULL | copied from the plan, so an old run renders without its commit |
| `needs` | JSONB NOT NULL | array of stage names; default `'[]'` |
| `status` | TEXT NOT NULL | default `'queued'` |
| `exit_code` | INTEGER | |
| `started_at`, `finished_at` | TIMESTAMPTZ | |
| `error` | TEXT | timeout, cancellation, skip reason, secret or executor error |
| `reused_from_run` | TEXT | FK `runs(id)` ON DELETE SET NULL; set when carried over by a restart |

Index `jobs_run_idx (run_id)`. There is no uniqueness on `(run_id, stage, name)`.

### steps

`id` TEXT PK; `job_id` FK `jobs(id)` ON DELETE CASCADE; `ordinal` INTEGER NOT NULL; `kind` (`run`, `anvil`, `riveter`, `warehouse`); `command` (redacted); `status` default `'queued'`; `exit_code`; `started_at`; `finished_at`. Unique `steps_job_ordinal_idx (job_id, ordinal)`. Rewritten as a whole when a job ends (`DELETE` then `INSERT`).

### logs

Append-only. `job_id` FK `jobs(id)` ON DELETE CASCADE, `seq` BIGINT, `stream` (`stdout`/`stderr`), `written_at`, `chunk` TEXT (the line). Primary key `(job_id, seq)`; inserts use `ON CONFLICT DO NOTHING`. Written in one transaction when the job ends.

### artifacts

The reference, not the bytes. `id` PK; `run_id` FK `runs(id)` ON DELETE CASCADE; `job_id` FK `jobs(id)` ON DELETE CASCADE; `kind` (`file` as written by conveyor); `name`; `version`; `uri`; `digest` (`sha256:<hex>`); `created_at`. Index `artifacts_run_idx (run_id)`.

### secrets

Sealed values. `id` PK; `repo_id` FK `repos(id)` ON DELETE CASCADE (NULL for an estate-wide secret); `name`; `nonce` BYTEA (24 bytes, fresh per write); `ciphertext` BYTEA; `created_by` FK `auth.users(username)` ON DELETE CASCADE; `created_at`; `updated_at`. Uniqueness is two partial indexes (NULLs are distinct in Postgres): `secrets_repo_name_idx (repo_id, name) WHERE repo_id IS NOT NULL` and `secrets_global_name_idx (name) WHERE repo_id IS NULL`. The seal context is `global:<name>` or `repo:<repo id>:<name>`, so a row copied between scopes or renamed fails to open.

### projects

`id` PK; `name`; `parent_id` FK `projects(id)` ON DELETE CASCADE; timestamps. Unique `projects_parent_name_idx (parent_id, name)` and `projects_root_name_idx (name) WHERE parent_id IS NULL`.

### credentials

`id` PK; `project_id` FK `projects(id)` ON DELETE CASCADE; `repo_id` FK `repos(id)` ON DELETE CASCADE; `name`; `kind`; `username`; `preview` (`••••…` + last four characters, in the clear); `nonce`, `ciphertext` (the token); `created_by` FK `auth.users` ON DELETE CASCADE; timestamps. Constraint `credentials_one_scope CHECK ((project_id IS NOT NULL) <> (repo_id IS NOT NULL))`. Unique partial indexes `credentials_project_idx (project_id)` and `credentials_repo_idx (repo_id)`: one credential per scope. Seal context `project:<id>` or `repo:<id>`.

### subscriptions

`id` PK; `username` FK `auth.users(username)` ON DELETE CASCADE; `repo_id` FK `repos(id)` ON DELETE CASCADE; `project_id` FK `projects(id)` ON DELETE CASCADE; `created_at`. Constraint `subscriptions_one_scope CHECK ((repo_id IS NULL) <> (project_id IS NULL))`. Unique partial indexes `(username, repo_id)` and `(username, project_id)`; lookup indexes on `repo_id` and `project_id`.

### notification_outbox

`id` PK; `run_id` FK `runs(id)` ON DELETE CASCADE; `username` (no FK); `template` (`conveyor.run.failed` / `conveyor.run.recovered`); `vars` TEXT (a JSON object); `attempts` INTEGER default 0; `next_attempt_at` default `NOW()`; `last_error`; `failed_at`; `created_at`. Unique `notification_outbox_once_idx (run_id, username, template)`; index `notification_outbox_due_idx (next_attempt_at) WHERE failed_at IS NULL`. A row is deleted once gatehouse answered for good; a row with `failed_at` is kept for a person to read.

### schedules

`id` PK; `repo_id` FK `repos(id)` ON DELETE CASCADE; `expression` TEXT; `timezone` TEXT default `'UTC'`; `next_run_at` TIMESTAMPTZ NOT NULL; `last_run_at`; timestamps. Unique `schedules_repo_expression_idx (repo_id, expression, timezone)`; index `schedules_due_idx (next_run_at)`.

## Behaviour that follows from the schema

* Deleting a realm user deletes the repositories that user registered (with their runs) and the secrets and credentials they created, through the `ON DELETE CASCADE` foreign keys.
* Deleting a repository removes its runs and everything under them. Deleting a project requires no children and no repository.
* Nothing in the service deletes old runs, jobs, steps, logs, artifact rows or `failed` outbox rows: the history grows until a person removes it. (Deletes in the code: secrets, credentials, subscriptions, schedules, completed outbox rows, repositories, projects, and a job's steps before they are rewritten.)
* Logs are stored only when a job ends; a job interrupted by a dead worker has none.
* After the janitor requeues a run, the next claim writes a fresh set of `jobs` rows for it without removing the previous attempt's (see the report: duplicated jobs).
