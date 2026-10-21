# Daybook Service

Daybook is an API-only task manager for the Forge estate. It provides a
personal inbox and user-owned custom project lists. It has no web UI, local
login. Gatehouse verifies the bearer token and the service scopes all rows to
its `sub` username. The token must include the `daybook` audience and the
`daybook:read` (list) or `daybook:write` (create, update, delete) grant; the
standard Gatehouse viewer template grants `read`, the editor template both.
There is no resource scoping - every user only ever sees their own rows.

## API

All routes are under `/api/v1` (plus the configured `BASE_PATH`) and require a
Gatehouse bearer token.

| Method | Route | Behavior |
|---|---|---|
| `GET` | `/api/v1/tasks` | List inbox tasks by default; pass `project_id` to list that project. Optional `completed=true/false`. |
| `POST` | `/api/v1/tasks` | Create `{ "content": "...", "project_id": null }`; null project means inbox. |
| `PUT` | `/api/v1/tasks/{id}` | Replace content, description, project, completion, sort order and due date. |
| `DELETE` | `/api/v1/tasks/{id}` | Delete an owned task. |
| `GET` | `/api/v1/projects` | List the caller's custom lists. |
| `POST` | `/api/v1/projects` | Create `{ "name": "...", "sort_order": 0 }`. |
| `PUT` | `/api/v1/projects/{id}` | Rename/reorder an owned list. |
| `DELETE` | `/api/v1/projects/{id}` | Delete an owned list and its tasks. |

Task objects include `id`, `project_id`, `content`, `description`,
`is_completed`, `sort_order`, `due_at`, `created_at` and `updated_at`. A task
cannot be attached to another user's project. The database enforces this as
well as the API's owner filters.

## Requirements and configuration

- Postgres with the `daybook` schema installed by Foundry.
- Gatehouse-issued JWTs accepted by the shared `AUTH_DB_SCHEMA` and
  `SERVICE_AUTH_ENABLED` settings.
- `DATABASE_URL`, `BASE_PATH`, `SERVER_ADDR`, `AUTH_DB_SCHEMA`, and
  `SERVICE_AUTH_ENABLED`, supplied through the standard service environment.

In local development, `foreman start daybook` brings up Gatehouse, Postgres,
installs the Daybook schema and starts the service on port `12443` at
`/daybook`.

## Tests

- `cargo test -p daybook-service` runs the fast unit suite; the database-backed
  integration cases skip unless `DAYBOOK_TEST_DATABASE_URL` is set.
- For the integration suite, install the `daybook` Foundry module into a
  throwaway Postgres database, then run
  `DAYBOOK_TEST_DATABASE_URL=<url> cargo test -p daybook-service --test integration`.
- `cargo run -p forge-bdd -- --service daybook` starts Gatehouse and Daybook
  and checks authentication, validation and API-only behavior over HTTP.
