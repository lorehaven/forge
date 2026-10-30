---
title: API
parent: Daybook
grand_parent: Services
nav_order: 1
---

The HTTP API of `daybook-service`, rendered from [openapi.yaml](openapi.yaml) (OpenAPI 3.1, checked against the route macros by `scripts/docs-check.py daybook`). All paths are under `BASE_PATH` (`/daybook`).

## Conventions

* **Authentication.** `Auth` wraps the prefix `{BASE_PATH}/api/v1` (including unmapped paths under it, which answer 401 rather than 404). It accepts `Authorization: Bearer <access token>` or the realm session cookie, verifies the signature against gatehouse's JWKS and checks that the token's session id is active in the session store. Failure: `401` with `WWW-Authenticate: Bearer` and no body. The `/auth/*` and `/ui/*` routes are public.
* **Permissions.** Each handler then checks the grant: reads need `daybook:read`, writes (POST, PUT, DELETE) need `daybook:write`; otherwise `403` with `{"error": "no read access to daybook"}` or `{"error": "no write access to daybook"}`. A grant of one does not imply the other. With `SERVICE_AUTH_ENABLED=false` both checks pass and the owner is `dev`.
* **Ownership.** Every query filters on `owner = token.sub`. Another user's id gives `404` (or `400` when it is a reference inside a body).
* **Errors.** `{"error": "<text>"}`. `500` carries the database error text.
* **Times** are RFC 3339 strings; ids are UUID v4 strings; unspecified values in responses are `null`, never absent.
* **Content type** of bodies: `application/json`.

Status codes by cause:

| Status | Body `error` |
|---|---|
| 400 | `content is required` / `name is required` / `color must be #rrggbb` / `a repeating task needs a due date` / `recurrence interval must be between 1 and 999` / `recurrence timezone is not a known IANA zone` / `recurrence weekdays only apply to a weekly rule` / `recurrence weekdays need the fixed schedule, not after_completion` / `recurrence weekdays are 1 (Monday) to 7 (Sunday)` / `a referenced project or label does not exist for this user` |
| 404 | `task not found` / `project not found` / `label not found` |
| 409 | `that name is already in use` (labels, unique ignoring case) |
| 503 | `daybook requires Postgres` (in-memory database) |

A body that is not valid JSON or lacks `content`/`name` is rejected by the `Json` extractor before the handler; its status and text come from `quench-http` and are not specified here.

## tasks

### GET /api/v1/tasks

List tasks. Permission `daybook:read`.

| Query | Type | Default | Meaning |
|---|---|---|---|
| `project_id` | string | none | A list's id. |
| `label_id` | string | none | Only tasks with this label. |
| `all` | bool | `false` | Do not restrict to the inbox. |
| `completed` | bool | none | Filter on `is_completed`. |

Selection (from the SQL): with no `project_id`, tasks are included if `all` is true, or if `label_id` is given, or if the task is in the inbox; with `project_id`, only that list. So: no params = inbox; `project_id` = that list; `label_id` = every list and the inbox (narrowed by `project_id` if also given); `all=true` = everything. Order: `sort_order`, `created_at`. `200` with an array of `Task`.

```http
GET /daybook/api/v1/tasks?all=true&completed=false
```

```json
[{
  "id": "6b0c...", "project_id": null, "content": "Pay the rent", "description": null,
  "is_completed": false, "sort_order": 0,
  "due_at": "2026-11-01T00:00:00Z", "due_all_day": true, "remind_at": "2026-10-31T09:00:00Z",
  "label_ids": [], "recurrence": {"unit": "month", "interval": 1, "weekdays": [], "mode": "fixed", "timezone": "Europe/Warsaw"},
  "created_at": "2026-10-30T10:00:00Z", "updated_at": "2026-10-30T10:00:00Z"
}]
```

### POST /api/v1/tasks

Create. Permission `daybook:write`. Body `TaskInput`:

| Field | Type | Default | Notes |
|---|---|---|---|
| `content` | string | required | trimmed; empty gives 400 |
| `description` | string or null | null | stored as sent (not trimmed) |
| `project_id` | string or null | null | null is the inbox |
| `sort_order` | int32 | 0 | |
| `due_at` | RFC 3339 or null | null | |
| `due_all_day` | bool | false | ignored without `due_at`; see below |
| `remind_at` | RFC 3339 or null | null | stored only: the service sends nothing |
| `label_ids` | string[] | `[]` | duplicates are ignored |
| `recurrence` | `Recurrence` or null | null | needs `due_at` |

`201` with the `Task`. Errors 400, 403.

### PUT /api/v1/tasks/{id}

Replace. Permission `daybook:write`. Body `TaskUpdateInput`: the same fields plus `is_completed` (bool, default false). Every field is replaced; leaving one out resets it to its default/null. `200` with the task as stored, `404` when it is not the caller's.

Roll-forward: in one transaction (the row is locked `FOR UPDATE`), if `is_completed` is true, the stored task was open, a `recurrence` is sent and the sent `due_at` is present and a next occurrence is found, then `due_at` becomes that occurrence, `remind_at` (if sent) is shifted by the same lead time, and `is_completed` is stored false. If no occurrence is found (reaching the 20 000-step limit), the task is completed. Reopening (`is_completed` false) never rolls.

### DELETE /api/v1/tasks/{id}

`204`, or `404`. Permission `daybook:write`.

## Due dates

With `due_all_day: true` the service stores midnight UTC of the UTC date of the sent `due_at` (`floor(seconds / 86400) * 86400`); clients read it back as that UTC date in any zone. Repeating all-day tasks roll by whole dates.

## Recurrence

```json
{"unit": "week", "interval": 2, "weekdays": [1, 4], "mode": "fixed", "timezone": "Europe/Warsaw"}
```

| Field | Rule |
|---|---|
| `unit` | `day`, `week`, `month`, `year` |
| `interval` | 1 to 999 |
| `weekdays` | default `[]`; only with `unit: week` and `mode: fixed`; each 1 (Monday) to 7; sorted and de-duplicated; empty means the weekday of the due date |
| `mode` | default `fixed`; `after_completion` counts from the completion day |
| `timezone` | default `UTC`; an IANA name known to `quench-tz` |

`fixed`: the first occurrence after both the due date and now, counted from the anchor (so finishing early moves past the occurrence just done, finishing late skips those already past). `after_completion`: `interval` units after today in the rule's zone, at the anchor's time of day. Months and years are computed from the anchor, not stepwise. Not supported: end dates, calendar rules such as "last working day".

## projects

### GET /api/v1/projects

`daybook:read`. Array of `Project` ordered by `sort_order`, `name`.

### POST /api/v1/projects

`daybook:write`. Body `{"name": string, "sort_order": int32 = 0}`; `name` trimmed, non-empty. `201` `Project` (`id`, `name`, `sort_order`, `created_at`).

### PUT /api/v1/projects/{id}

`daybook:write`. Same body; `200` `Project`; `404 project not found`.

### DELETE /api/v1/projects/{id}

`daybook:write`. Deletes the list and, by cascade, its tasks. `204` / `404`.

## labels

### GET /api/v1/labels

`daybook:read`. Array of `Label` (`id`, `name`, `color`, `created_at`) ordered by lower-case name, then id.

### POST /api/v1/labels

`daybook:write`. Body `{"name": string, "color": "#rrggbb" = "#888888"}`. The name is trimmed; the colour must match `#` plus six hex digits (either case) and is stored lower-case. `201` `Label`; `409` when the name exists (any case); `400` for an empty name or bad colour.

### PUT /api/v1/labels/{id}

`daybook:write`. Same body; `200`, `404 label not found`, `409`.

### DELETE /api/v1/labels/{id}

`daybook:write`. Removes the label from all tasks; the tasks stay. `204` / `404`.

## auth

### POST /auth/login and POST /auth/refresh

Public. The JSON body is forwarded unchanged to `{GATEHOUSE_URL}/api/v1/auth/login` or `.../refresh`; the response is gatehouse's status and body verbatim, with `content-type: application/json` and `cache-control: no-store`. If gatehouse is unreachable: `502` `{"error":"cannot reach the sign-in service"}`. If the status gatehouse sent cannot be represented: `502`. This lets the browser client call only the origin it came from (`/daybook/auth/login`).

## ui

Public GET routes, served by `forge_web::Client` (see [Client](client.md)):

| Route | Content |
|---|---|
| `/ui` | The HTML page that starts the client |
| `/ui/assets/quench.js` | Loader script |
| `/ui/assets/app.css` | `daybook_web::stylesheet()` |
| `/ui/assets/shell.js` | forge shell script (top bar) |
| `/ui/assets/favicon.png` | `services/daybook/icon.png`, compiled in |
| `/ui/assets/app.wasm` | `$WEB_DIR/app.wasm`; 404 if missing |

Also mounted by `quench-starter` (not in the OpenAPI document, because they are not in this service's sources): `/health`, `/health/live`, `/health/ready`, the swagger and base-path redirect routes.
