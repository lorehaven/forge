---
title: Workbench
parent: Services
nav_order: 13
has_children: true
---

Workbench is the estate's issue tracker: projects, issues with a fixed five-state workflow, comments, project labels and typed links between issues. It keeps no users of its own: identities come from Gatehouse, and the assignee picker reads the realm's `auth.users`. The browser client is a Kanban board and an issue page.

## Crates

| Crate | Path | Version | Role |
|---|---|---|---|
| `workbench-core` | `services/workbench/core` | 0.1.0 | Wire model (`Project`, `Issue`, `Comment`, `RelatedIssues`, `User`, `Me`, create/update bodies), form drafts (`ProjectDraft`, `IssueDraft`, `CommentDraft`, `LinkDraft`), board logic (`columns`, `moved`, `status_key`, `short_time`). No I/O. |
| `workbench-app` | `services/workbench/app` | 0.1.0 | Screens and the `Request`/`Msg` protocol (`quench-ui`). |
| `workbench-web` | `services/workbench/web` | 0.1.0 | `wasm32` module: `Client::execute` turns requests into API calls over a `Transport`; `fetch` transport. |
| `workbench` | `services/workbench/service` | 0.3.2 | The server: API, SSO routes, page and asset routes. Binary `workbench-service`. |

The foundry module `workbench` 0.1.0 (schema `workbench`) owns the tables.

## Facts

* Foreman: HTTPS 10443 (see [Running the estate locally](../../guides/running-locally.md)), `BASE_PATH=/workbench`, `needs = ["gatehouse"]`.
* Dependencies: Postgres (required: `DATABASE_URL`; without it the process refuses to start unless `ALLOW_IN_MEMORY_DB=true`, and then every data call answers `503`), Gatehouse (JWKS, SSO, readiness), Redis for sessions (in-process when unset).
* Authentication: all of `/api/v1` behind `Auth`; grants checked per route, see [Permissions](permissions.md).
* The statuses are `blocked`, `todo`, `in-progress`, `done`, `rejected`; any status may follow any other.

## Pages

| Page | Content |
|---|---|
| [API](api.md), [openapi.yaml](openapi.yaml) | 26 API operations and 17 browser routes. |
| [Permissions](permissions.md) | Grants, project-scoped grants, which route checks what. |
| [Configuration](configuration.md) | Environment variables. |
| [Data](data.md) | Tables, constraints, the issue numbering. |
| [Operations](operations.md) | Running, health, failure modes, tests. |
| [Client](client.md) | Screens, requests, i18n keys. |

Shared behaviour: [Common service runtime](../common-runtime.md).
