---
title: Permissions
parent: Conveyor
grand_parent: Services
nav_order: 6
---

What a caller needs for each route. The catalog entry (`services/gatehouse/service/config/permissions.toml`): service `conveyor`, label `Conveyor`, actions `read` and `write`, resource type `project`. Mechanics of tokens and roles are in [Authentication and permissions](../../guides/authentication.md).

## Grants

| Grant (a role entry in the token's `scope`) | Meaning |
|---|---|
| a wildcard role (admin, or a client-credentials `service` token) | everything |
| `conveyor:read` / `conveyor:write` | the action on every project, and the only grant that covers estate-wide secrets and creating root nodes |
| `conveyor:project:<id>:read` / `:write` | the action on that node of the project tree **and on everything nested under it** |

Templates: `viewer` has `conveyor:read`, `editor` has `conveyor:read` and `conveyor:write` (from `permissions.toml`). `read` and `write` are independent: neither implies the other.

`can_on_project(claims, project, action)`: true if auth is off; false with no claims; true if `claims.can("conveyor", action)`; otherwise true if any project on the chain from `project` up to its root has the grant `conveyor:project:<that id>:<action>`. `granted_project_ids` lists the nodes a token holds directly (it is then expanded to descendants with a recursive query when a list has to be filtered).

A repository belongs to the project in `repos.project_id`; "read/write on a repository" means on that project. Deleting or moving a node changes who can reach what at once; nothing is cached.

## Per route

| Route | Check |
|---|---|
| `POST /projects` | write on `parent_id`; without a parent, blanket write |
| `GET /projects` | none beyond authentication |
| `GET /projects/{id}` | read on the node |
| `PATCH`, `DELETE /projects/{id}` | write on the node (and on the destination for a move) |
| `POST /repos` | write on `project_id` |
| `GET /repos`, `GET /runs` without `repo_id`, `GET /credentials`, `GET /scan`, `GET /overview/*` | filtered to readable projects; never 403 |
| `GET /repos/{id}` and sub-reads (`runs`, `schedules`, `stages`, `scan*`, `secrets`, `credentials`) | read on the repository's project |
| `PATCH`, `DELETE /repos/{id}`, `POST /repos/{id}/enabled`, `PUT/DELETE` of secrets and credentials, `POST /repos/{id}/runs` | write on the repository's project |
| `POST /runs/{id}/cancel`, `/restart` | write on the run's repository's project |
| `GET /runs/{id}`, `GET /jobs/{id}/*` | read on the run's/job's repository's project |
| `GET`, `PUT`, `DELETE /secrets...` (estate-wide) | blanket read / blanket write only |
| `GET`, `PUT`, `DELETE /projects/{id}/credentials` | read / write / write on the project |
| `PUT /repos/{id}/subscription`, `PUT /projects/{id}/subscription` | read on the target |
| `GET /subscriptions`, `DELETE .../subscription` | any authenticated caller; always the caller's own |
| `GET /access` | any authenticated caller |
| `POST /webhooks/{provider}` | none: the HMAC signature |

If a run's repository cannot be found, `GET /runs/{id}`, cancel and restart answer `403` (`no read access to this run` / `no write access to this run`) to everyone, the blanket grant included. The schema deletes a repository's runs with it (`runs.repo_id ... ON DELETE CASCADE`), so this is only reachable by a race. A repository credential whose repository is gone is left out of `GET /credentials`.

## Machine identities

* `conveyor-gatehouse` (client credentials, audience `gatehouse`): conveyor's own identity when it asks gatehouse to send mail.
* `conveyor-publish` (client credentials, audience `warehouse` only): the identity pipelines publish rivet packages with, and, by default (`WAREHOUSE_CLIENT_ID`), the identity conveyor itself uses to upload and download artifacts when `WAREHOUSE_CLIENT_SECRET` is set.
* A pipeline or script that calls conveyor's API holds a token of whatever client it uses; its grants are the token's roles like anyone's.

## Webhook secrets

Not a grant but an access control of its own: the repository secret `WEBHOOK_SECRET` (sealed with `CONVEYOR_SECRET_KEY`), else the estate secret `CONVEYOR_WEBHOOK_SECRET`. A compromised hook of one repository cannot forge deliveries of another when each has its own.
