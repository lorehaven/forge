---
title: Configuration
parent: Workbench
grand_parent: Services
nav_order: 2
---

Platform variables (`BASE_PATH`, `SERVER_ADDR`, `SERVER_HTTP_REDIRECT_ADDR`, `SERVICE_NAME`, `SERVICE_AUTH_ENABLED`, `GATEHOUSE_URL`, `GATEHOUSE_CLIENT_ID`, `GATEHOUSE_CLIENT_SECRET`, `GATEHOUSE_TLS_VERIFY`, `AUTH_*`, `REDIS_URL`, `RUST_LOG`, `LOG_*`) are described in [Common service runtime](../common-runtime.md). Workbench reads these in addition:

| Variable | Default | Where | Meaning |
|---|---|---|---|
| `DATABASE_URL`, then `POSTGRES_URL` | empty | `DbWrapper::init_env` | Postgres. Empty and `ALLOW_IN_MEMORY_DB` not true: the process panics `no database configured`. |
| `ALLOW_IN_MEMORY_DB` | `false` | `DbWrapper::init` | Tests only. With an in-memory database every data route answers `503` with `workbench needs Postgres; ...`. |
| `DB_SCHEMA` | `workbench` | `domain/db.rs` `schema()` | Schema of the tables; interpolated into SQL unchecked. |
| `DB_POOL_MAX_SIZE` | `5` | `quench-db` `PostgresDb::new` | Pool size; an unparsable value gives 5. |
| `AUTH_DB_SCHEMA` | `auth` | `quench_auth::domain::realm::auth_schema()` | Schema of `users`, read for `GET /api/v1/users`. The foreign keys of the tables are fixed at install time by the foundry variable `auth_schema` (default `auth`). |
| `WEB_DIR` | `web` | `forge-web` | Directory with `app.wasm`. |
| `WORKBENCH_TEST_DATABASE_URL` | none | integration tests | Tests only; they skip without it. |

The service has no config file and no `FEATURE_*` flags. The secret is `GATEHOUSE_CLIENT_SECRET` (names only). Under foreman the service gets `GATEHOUSE_CLIENT_SECRET` and the common defaults (`DATABASE_URL=postgres://postgres:postgres@localhost:5432/postgres`).
