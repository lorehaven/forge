---
title: Permissions
parent: Workbench
grand_parent: Services
nav_order: 6
---

Source: `service/src/routers/api/authz.rs`. The Gatehouse catalog entry is `workbench` with actions `read` and `write` and the resource type `project` (see [Authentication and permissions](../../guides/authentication.md)). Templates: `viewer` has `workbench:read`, `editor` has `read` and `write`.

## Grant forms

| Grant | Meaning |
|---|---|
| `workbench:read`, `workbench:write` | Blanket: every project. |
| `workbench:project:<project id>:read`, `...:write` | One project. The id is the project's `id` column. |

`Claims::can("workbench", action)` is also true for any wildcard role (`admin`, `service`). There is no ordering between actions: `write` does not imply `read`.

## Checks

`can_on_project(claims, config, project_id, action)`: true when authentication is off (`SERVICE_AUTH_ENABLED=false`); false when there are no claims; otherwise true if `can("workbench", action)` or `can("workbench", "project:<id>:<action>")`. `can_unscoped` (used to create a project) accepts only `can("workbench", action)`. For an issue, comment, label or link the project is the one the record belongs to: the service reads the record first, so an unknown id answers `404` before any permission is checked (with the exception of the project-level routes, which check first).

| Route | Check |
|---|---|
| `POST /api/v1/projects` | unscoped `write` only. |
| `GET /api/v1/projects` | no 403: blanket `read` lists all; otherwise only projects with a scoped `read` grant (`granted_project_ids`, which parses `project:<id>:read` entries by splitting at the last `:`). With authentication off (no claims) nothing is filtered. |
| project routes (`GET/PUT/DELETE /projects/{id}`, issues and labels under a project) | `read` or `write` on that project. The order is: validate the body, then the grant, then existence for `PUT /projects/{id}`; grant first for the others. |
| issue, comment, label, link routes by id | `read` (GET) or `write` (everything else) on the owning project. |
| `GET /api/v1/issues?key=` | `read` on the found issue's project, after the lookup. |
| `GET /api/v1/me`, `GET /api/v1/users` | any authenticated identity. |
| `/ui/*` pages | a valid session cookie only; no workbench grant. A signed-in person without any grant sees the pages and an empty or refused API (the client shows "this account may not see Workbench: ask for access in gatehouse" on a 403 read). |

`reporter` and `author` are the token `sub`; with authentication off they are `dev`.

Errors are `{"error": "<text>"}`: `no read access here`, `no write access here`, `no read access to this issue`, `no write access to this issue`, `no write access to this comment`, `no write access to this label`, `no write access to this link`.
