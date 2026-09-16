# Workbench Service

Workbench is Forge's task-management service: a Jira/YouTrack-shaped issue tracker — projects, issues (with sub-issue nesting), comments, labels and cross-issue links — with no local users of its own, delegating login entirely to Gatehouse. Binary and crate name: `workbench-service` (`docker/workbench-service`).

## Features

- **Projects and issues** — a project is a flat namespace (`key`, `name`, `description`); an issue belongs to one project, optionally nests under a `parent_id`, and gets a transactionally-assigned, gapless `seq` displayed as `{project.key}-{seq}` (e.g. `WB-3`) — computed from `seq` on read, never stored redundantly.
- **Fixed v1 workflow** — five statuses (`blocked`, `todo`, `in-progress`, `done`, `rejected`), no per-project configuration yet; `blocked`/`rejected` bracket the three states that represent forward progress.
- **Comments and labels** — comments are a flat, timestamped list per issue; labels are project-scoped and attached/detached from an issue many-to-many, idempotently (attaching twice is a no-op, not an error).
- **Issue links** — typed cross-issue relationships (e.g. "blocks", "relates to") independent of the parent/child nesting above.
- **Board view** — `/ui/projects/{id}/board` groups a project's issues by status for a Kanban-style read, HTMX-driven for in-place transitions and issue creation.
- **No local identity** — every `/ui/*` auth route (`login`, `logout`, `callback`, `refresh`, `status`) delegates straight to Gatehouse's SSO client; Workbench never sees a password and holds no user table beyond the audit-column usernames (`reporter`, `assignee`, `author`) an authenticated session supplies.

## API routes

| Method | Path | Purpose |
|---|---|---|
| `POST` | `/api/v1/projects` | create a project |
| `GET` | `/api/v1/projects` | list projects |
| `GET` | `/api/v1/projects/{id}` | read one project |
| `PUT` | `/api/v1/projects/{id}` | update a project |
| `DELETE` | `/api/v1/projects/{id}` | delete a project |
| `POST` | `/api/v1/projects/{id}/issues` | create an issue in a project |
| `GET` | `/api/v1/projects/{id}/issues` | list a project's issues |
| `GET` | `/api/v1/issues/{id}` | read one issue |
| `PUT` | `/api/v1/issues/{id}` | update an issue |
| `POST` | `/api/v1/issues/{id}/transition` | move an issue to a new status |
| `DELETE` | `/api/v1/issues/{id}` | delete an issue |
| `GET` | `/api/v1/issues/{id}/labels` | labels attached to an issue |
| `POST`/`DELETE` | `/api/v1/issues/{id}/labels/{label_id}` | attach/detach a label |
| `POST` | `/api/v1/projects/{id}/labels` | create a project label |
| `GET` | `/api/v1/projects/{id}/labels` | list a project's labels |
| `DELETE` | `/api/v1/labels/{id}` | delete a label |
| `POST` | `/api/v1/issues/{id}/comments` | add a comment |
| `GET` | `/api/v1/issues/{id}/comments` | list an issue's comments |
| `DELETE` | `/api/v1/comments/{id}` | delete a comment |
| `POST` | `/api/v1/issues/{id}/links` | create an issue link |
| `GET` | `/api/v1/issues/{id}/links` | list an issue's links |
| `DELETE` | `/api/v1/issue-links/{id}` | delete an issue link |
| `GET` | `/ui/home`, `/ui/home/` | project list, entry point after login |
| `POST` | `/ui/projects` | create a project (HTMX form) |
| `GET` | `/ui/projects/{id}/board` | Kanban board for a project |
| `POST` | `/ui/projects/{id}/issues` | create an issue from the board |
| `POST` | `/ui/projects/{project_id}/issues/{issue_id}/transition` | move an issue from the board |
| `GET`/`POST` | `/ui/issues/{id}` | issue detail page / inline edit |
| `POST` | `/ui/issues/{id}/comments` | add a comment (HTMX fragment) |
| `POST` | `/ui/issues/{id}/links`, `/ui/issues/{id}/links/{link_id}/delete` | link management (HTMX fragments) |
| `GET` | `/ui/assets/{path:.*}` | generated CSS/static assets |
| `GET`/`POST` | `/ui/login`, `/ui/logout`, `/ui/auth/callback`, `/ui/status`, `/ui/refresh` | delegate entirely to Gatehouse SSO |

## Architecture

### `Auth` over the whole API, authorization per route

`routers::api::wrap_auth` wraps `Auth` (from `quench-auth`) around all of `/api/v1`, including its own 404 fallback — an unmapped path under the API answers `401`, not `404`, so an unauthenticated prober can't map the surface. There is no blanket `RequireWrite`: each route checks `authz::can_on_project`/`can_unscoped` itself against the caller's claims, since a workbench write is either scoped to a specific project or (creating a project) unscoped.

### Permissions: `workbench:<action>` or `workbench:project:{id}:<action>`

Registered in Gatehouse's catalog as `[services.workbench]` with `actions = ["read", "write"]` and `resource_types = ["project"]`. A grant is either the blanket `workbench:write` or the project-scoped `workbench:project:{id}:write` — `authz::can_on_project` checks both, `authz::granted_project_ids` reverses the scoped form to answer "which projects can this caller act on" without a database round trip. Projects are flat, so a scoped grant checks directly against `project_id`; there's no ancestor walk the way a nested resource would need. With `SERVICE_AUTH_ENABLED`/`auth_enabled` off, every check passes — the realm-wide dev bypass every other service also has.

### Issue numbering

`seq` is assigned inside the same transaction that inserts the issue, scoped per `project_id`, so concurrent creates in one project still get gapless, unique sequence numbers (and creates in different projects never contend). The display key (`{project.key}-{seq}`, e.g. `WB-3`) is computed from `seq` at read time via `Issue::key`, not stored — one less place for a rename or migration to leave it stale.

### No local users

Unlike Gatehouse, Workbench keeps no `users` table: `reporter`/`assignee`/`author` columns just store the authenticated subject's username string. `/ui/login` and friends (`routers/ui/pages/auth.rs`) are thin wrappers around `quench_auth::http::routers::ui::pages::auth`'s delegation helpers (`login_delegation`, `auth_callback`, `logout_delegation`, `refresh_delegation`) — the same no-local-login shape Sage and Switchboard use.

## Requirements

- Postgres for the `workbench` schema (projects, issues, comments, labels, issue_links) — installed via foundry's `workbench` catalog module.
- **Gatehouse** — JWT/session auth and SSO delegation for both the API and UI; Workbench has no login form of its own.
- Redis/`REDIS_URL` for the session store `quench-auth`'s SSO client needs, same as any other relying party.

## Configuration

| Variable | Purpose |
|---|---|
| `DATABASE_URL` | Postgres connection string |
| `DB_SCHEMA` | schema name, default `workbench` |
| `DB_POOL_MAX_SIZE` | connection pool size |
| `BASE_PATH` | path prefix every route is scoped under |
| `SERVER_ADDR` / `SERVER_HTTP_REDIRECT_ADDR` | listen address / HTTP→HTTPS redirect address |
| `GATEHOUSE_URL` | where `/ui/login` etc. redirect to |
| `GATEHOUSE_CLIENT_ID` / `GATEHOUSE_CLIENT_SECRET` | this service's OAuth client identity for the authorization-code + PKCE round trip |
| `AUTH_DB_SCHEMA` | realm schema name Workbench reads to verify tokens, default `auth` |
| `REDIS_URL` | session store |
| `SERVICE_NAME` | used in startup log lines |
| `SERVICE_AUTH_ENABLED` | realm-wide dev bypass when `false` — every `authz` check passes |
| `LOG_SKIP_PREFIXES` | comma-separated path prefixes the request logger won't log on success |
| `RUST_LOG` | verbosity |

In local dev (`foreman.toml`) Workbench declares `needs = ["gatehouse"]` — nothing else in the estate depends on it, and it depends on nothing beyond the realm.

## Testing

Unit tests (`tests/unit/`: `authz_tests.rs`, `authz_request_tests.rs`) cover the permission logic above with no database. Integration tests (`tests/integration/`: `project_tests.rs`, `issue_tests.rs`, `comment_and_label_tests.rs`, `realm_users_tests.rs`, `routers_api_tests.rs`, `routers_ui_pages_tests.rs`) exercise real CRUD and HTTP routes against Postgres, and are skipped — not failed — unless `WORKBENCH_TEST_DATABASE_URL` is set, the same convention Conveyor's own Postgres-backed tests use. Run with `cargo test -p workbench-service` or `foreman test workbench`.

[Home](../README.md)
