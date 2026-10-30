---
title: Data
parent: Workbench
grand_parent: Services
nav_order: 3
---

Schema `workbench`, owned by the foundry module `workbench` 0.1.0 (requires `auth@^0.2`, `quench-core@^0.1`; variable `auth_schema = "auth"`). The service has no migrations of its own and creates nothing at start. Migrations in order (all `since = "0.1.0"`):

| Id | Creates |
|---|---|
| `0001-projects-and-issues` | `projects`, unique index `projects_key_idx`; `issues`, indexes `issues_project_seq_idx` (unique), `issues_project_status_idx`, `issues_parent_idx` (partial, `parent_id IS NOT NULL`) |
| `0002-comments` | `comments`, index `comments_issue_idx (issue_id, created_at)` |
| `0003-labels` | `labels`, unique index `labels_project_name_idx (project_id, name)`; `issue_labels`, index `issue_labels_label_idx` |
| `0004-issue-fields` | `issues.estimate INTEGER`; `issue_links` with constraints, indexes `issue_links_unique_idx` (unique) and `issue_links_linked_issue_idx` |

## Tables

`projects`: `id TEXT PK`, `key TEXT NOT NULL` (unique), `name TEXT NOT NULL`, `description TEXT`, `created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()`, `updated_at` same.

`issues`: `id TEXT PK`; `project_id TEXT NOT NULL REFERENCES projects ON DELETE CASCADE`; `parent_id TEXT REFERENCES issues ON DELETE CASCADE`; `seq INTEGER NOT NULL` (unique with `project_id`); `kind TEXT NOT NULL DEFAULT 'task'`; `title TEXT NOT NULL`; `description TEXT`; `status TEXT NOT NULL DEFAULT 'todo'`; `priority TEXT NOT NULL DEFAULT 'medium'`; `assignee TEXT REFERENCES auth.users(username) ON DELETE SET NULL`; `reporter TEXT NOT NULL REFERENCES auth.users(username) ON DELETE CASCADE`; `estimate INTEGER`; `created_at`, `updated_at` TIMESTAMPTZ. There is no CHECK constraint on `kind`, `status` or `priority`; the API validates `status` only.

`comments`: `id TEXT PK`, `issue_id TEXT NOT NULL REFERENCES issues ON DELETE CASCADE`, `author TEXT NOT NULL REFERENCES auth.users ON DELETE CASCADE`, `body TEXT NOT NULL`, `created_at`.

`labels`: `id TEXT PK`, `project_id TEXT NOT NULL REFERENCES projects ON DELETE CASCADE`, `name TEXT NOT NULL`, `color TEXT NOT NULL DEFAULT '#888888'`, `created_at`.

`issue_labels`: `(issue_id, label_id)` primary key, both `NOT NULL REFERENCES ... ON DELETE CASCADE`.

`issue_links`: `id TEXT PK`, `issue_id` and `linked_issue_id` `NOT NULL REFERENCES issues ON DELETE CASCADE`, `kind TEXT NOT NULL` with CHECK `issue_links_kind_check` (`'blocks'` or `'relates_to'`), CHECK `issue_links_no_self_link` (`issue_id <> linked_issue_id`), `created_at`.

Because `reporter` and `author` have `ON DELETE CASCADE` to `auth.users`, deleting a user deletes their issues and comments; `assignee` is set to null.

## Issue numbering

`seq` is assigned in the creating transaction: `pg_advisory_xact_lock(hashtextextended(project_id, 0))`, then `COALESCE(MAX(seq), 0) + 1` for the project, then the insert. Numbers are per project; deleting the newest issue lets its number be reused. The key `WB-4` is `{projects.key}-{seq}` and is not stored. Ids are random UUIDs rendered as text.

## Link semantics

A row `(issue_id, linked_issue_id, 'blocks')` means `issue_id` blocks `linked_issue_id`. `GET .../links` reads it as `blocks` for `issue_id` and as `blocked_by` for `linked_issue_id`. A `relates_to` row appears in `relates_to` for both issues. Redis, files: none.
