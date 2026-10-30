---
title: Data
parent: Daybook
grand_parent: Services
nav_order: 3
---

The service owns the Postgres schema `daybook` (name from `DB_SCHEMA`). The tables are created by the Foundry module `daybook` (`services/foundry/service/migrations/daybook/`), not by the service. It keeps nothing in Redis itself (Redis holds gatehouse sessions only).

## Foundry module

`module.toml`: name `daybook`, version `0.3.0`, default schema `daybook`, requires `auth@^0.2` and `quench-core@^0.1`, variable `auth_schema = "auth"`. (The service crate is at 0.3.2; the module version tracks the last migration, `since = "0.3.0"`.)

## Migrations, in order

| Id | Since | Effect |
|---|---|---|
| `0001-inbox-and-projects` | 0.1.0 | Creates `projects` and `tasks` (below) and the indexes `projects_owner_order_idx (owner, sort_order, name)` and `tasks_owner_project_order_idx (owner, project_id, is_completed, sort_order, created_at)`. |
| `0002-labels-and-reminders` | 0.2.0 | Adds `tasks.remind_at`; creates `labels`, unique index `labels_owner_name_idx (owner, lower(name))`, unique index `tasks_id_owner_idx (id, owner)`, `task_labels`, index `task_labels_label_idx (label_id)`. |
| `0003-recurrence` | 0.2.0 | Adds the six `recurrence_*` columns and the check `tasks_recurrence_complete`. |
| `0004-due-all-day` | 0.3.0 | Adds `tasks.due_all_day`. |

## Tables

`projects`

| Column | Type | Constraint |
|---|---|---|
| `id` | TEXT | primary key (UUID v4 text) |
| `owner` | TEXT | not null, references `auth.users(username)` on delete cascade |
| `name` | TEXT | not null |
| `sort_order` | INTEGER | not null, default 0 |
| `created_at` | TIMESTAMPTZ | not null, default `NOW()` |

Also `UNIQUE (id, owner)`.

`tasks`

| Column | Type | Constraint |
|---|---|---|
| `id` | TEXT | primary key |
| `owner` | TEXT | not null, references `auth.users(username)` on delete cascade |
| `project_id` | TEXT | null is the inbox; `FOREIGN KEY (project_id, owner)` references `projects(id, owner)` on delete cascade |
| `content` | TEXT | not null |
| `description` | TEXT | |
| `is_completed` | BOOLEAN | not null, default false |
| `sort_order` | INTEGER | not null, default 0 |
| `due_at` | TIMESTAMPTZ | |
| `created_at`, `updated_at` | TIMESTAMPTZ | not null, default `NOW()`; `updated_at` is set to `NOW()` on every update |
| `remind_at` | TIMESTAMPTZ | 0002 |
| `recurrence_unit` | TEXT | check in (`day`,`week`,`month`,`year`) |
| `recurrence_interval` | INTEGER | check between 1 and 999 |
| `recurrence_weekdays` | SMALLINT[] | not null, default `{}` |
| `recurrence_mode` | TEXT | check in (`fixed`,`after_completion`) |
| `recurrence_timezone` | TEXT | |
| `recurrence_anchor` | TIMESTAMPTZ | the due date the schedule is counted from |
| `due_all_day` | BOOLEAN | not null, default false (0004) |

Constraint `tasks_recurrence_complete`: either `recurrence_unit`, `_interval`, `_mode`, `_timezone` and `_anchor` are all null, or all not null (`_weekdays` is not part of it).

`labels`: `id` TEXT pk, `owner` TEXT not null (references `auth.users(username)` cascade), `name` TEXT not null, `color` TEXT not null default `#888888`, `created_at`, `UNIQUE (id, owner)`.

`task_labels`: `task_id`, `label_id`, `owner` (all TEXT not null), primary key `(task_id, label_id)`, foreign keys `(task_id, owner)` to `tasks(id, owner)` and `(label_id, owner)` to `labels(id, owner)`, both on delete cascade. Because both keys carry the owner, the database refuses a task that carries another user's label (SQL state `23503`, answered as 400).

## Anchor semantics

`recurrence_anchor` is set to the requested `due_at` whenever the rule is created, changed, or the due date or `due_all_day` is changed by a client; an update that leaves rule, due date and all-day flag as stored keeps the stored anchor. A roll (completion) keeps the anchor, so a monthly task on the 31st returns to the 31st after a short month. Creating a task sets the anchor to its (normalised) `due_at` when a rule is present.

## All-day storage

`normalize_due`: with `due_all_day` true and a `due_at`, the stored value is `floor(unix_seconds / 86400) * 86400`, i.e. midnight UTC of the sent instant's UTC date. `due_all_day` without `due_at` is stored as false.

## Client-side files

Programs keep files in `Store::standard()`: `$XDG_DATA_HOME/daybook` (absolute path only), else `$HOME/.local/share/daybook`, else the system temp directory + `/daybook`. Files are written to `<name>.tmp`, fsynced, and renamed.

| File | Mode | Content |
|---|---|---|
| `session` | 0600 (unix) | the refresh token and a newline |
| `settings` | default | lines `server=<https://host>`, `user=<name>`, `list=<key>`; `<key>` is `inbox`, `project:<id>` or `label:<id>`. Unknown lines are ignored. |
| `widget.json` | default | the widget snapshot (below); removed on sign-out |
| `launch` | | written by the Kotlin host; read and deleted by the program (see [Client](client.md)) |

### Widget snapshot (`widget.json`)

One JSON object (`daybook_core::widget::snapshot`):

```json
{
  "version": 1,
  "generated": 1793000000000,
  "lists": [
    {"id": "inbox", "name": "Inbox", "kind": "inbox"},
    {"id": "p:<project id>", "name": "Home", "kind": "project"},
    {"id": "l:<label id>", "name": "errand", "kind": "label", "color": "#22c55e"}
  ],
  "tasks": [
    {"id": "...", "content": "...", "project": null, "labels": ["<label id>"], "due": 1793000000000, "allDay": false, "repeats": false}
  ]
}
```

`generated` and `due` are milliseconds since the epoch; `project` and `due` are `null` when absent. Open tasks only, in display order (`order_tasks`), at most `MAX_TASKS` = 300. List ids are `inbox`, `p:<id>`, `l:<id>` (`list_id`/`parse_list_id`).

The browser client keeps `refresh` (refresh token) and `language` in `localStorage`.
