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
| `GET` | `/api/v1/tasks` | List inbox tasks by default; pass `project_id` to list that project. Pass `label_id` to list every task with that label across the inbox and all projects (add `project_id` to narrow it). Pass `all=true` to list every task regardless of list. Optional `completed=true/false`. |
| `POST` | `/api/v1/tasks` | Create `{ "content": "...", "project_id": null, "due_at": null, "remind_at": null, "label_ids": [], "recurrence": null }`; null project means inbox. |
| `PUT` | `/api/v1/tasks/{id}` | Replace content, description, project, completion, sort order, due date, reminder, labels and repeat rule. Omitting `remind_at`, `label_ids` or `recurrence` clears them. Completing a repeating task rolls it forward instead (see below). |
| `DELETE` | `/api/v1/tasks/{id}` | Delete an owned task. |
| `GET` | `/api/v1/projects` | List the caller's custom lists. |
| `POST` | `/api/v1/projects` | Create `{ "name": "...", "sort_order": 0 }`. |
| `PUT` | `/api/v1/projects/{id}` | Rename/reorder an owned list. |
| `DELETE` | `/api/v1/projects/{id}` | Delete an owned list and its tasks. |
| `GET` | `/api/v1/labels` | List the caller's labels, by name. |
| `POST` | `/api/v1/labels` | Create `{ "name": "...", "color": "#rrggbb" }`; colour defaults to `#888888`. Names are unique per user, ignoring case (`409` otherwise). |
| `PUT` | `/api/v1/labels/{id}` | Rename/recolour an owned label. |
| `DELETE` | `/api/v1/labels/{id}` | Delete an owned label; it is removed from every task, the tasks stay. |

Task objects include `id`, `project_id`, `content`, `description`,
`is_completed`, `sort_order`, `due_at`, `remind_at`, `label_ids` (ordered by
label name), `created_at` and `updated_at`. Label objects have `id`, `name`,
`color` and `created_at`. Labels belong to the user, not to a project, so one
label can tag tasks in the inbox and in any list. `remind_at` is only stored:
the service sends nothing, so clients schedule their own notification from it.
A task cannot be attached to another user's project or label. The database enforces this as
well as the API's owner filters.

## Repeating tasks

A task with a `recurrence` repeats. Completing it does not close it: the service moves `due_at` to the next
occurrence, moves `remind_at` with it (same lead time), and leaves `is_completed` false. The `PUT` response is
the rolled task, so a client sees it simply came back open with a later date. This happens in the one write
path every client uses, so a client that knows nothing about recurrence still keeps the series going, and two
devices completing the same occurrence land on the same next date.

```json
"recurrence": { "unit": "week", "interval": 2, "weekdays": [1, 4], "mode": "fixed", "timezone": "Europe/Warsaw" }
```

| Field | Meaning |
|---|---|
| `unit` | `day`, `week`, `month` or `year`. |
| `interval` | Every N units, 1-999. |
| `weekdays` | Optional, weekly + `fixed` only: ISO days (1 = Monday ... 7 = Sunday). Empty means the weekday the task is due on. With `interval` 2 it is every second week. |
| `mode` | `fixed` (default): counted from the schedule - a Monday task stays on Mondays however late it is done; finishing early moves past the occurrence you just did, finishing late skips occurrences already past. `after_completion`: N units after the day it is completed, at the original time of day. |
| `timezone` | IANA zone the rule is evaluated in (default `UTC`). This is what keeps a 09:00 task at 09:00 across a clock change; a time that does not exist that day (clocks going forward) lands an hour later. |

A repeating task needs a `due_at` (`400` otherwise). Months and years are counted from the date the schedule
was set, not step by step, so a task on the 31st is on the 28th in February and back on the 31st in March. That
date is restarted whenever the rule or the due date is changed by hand; a roll never changes it. Not supported:
end dates, or calendar rules such as "last working day of the month".

## Requirements and configuration

- Postgres with the `daybook` schema installed by Foundry.
- Redis (`REDIS_URL`): the session store. Gatehouse tokens carry a session id, and every request is
  rejected unless that session is still active there.
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
