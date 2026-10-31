---
title: Conveyor
parent: Services
nav_order: 1
has_children: true
permalink: /services/conveyor/
---

Conveyor is the estate's CI service. A webhook (or a person, or a clock) queues a run of a registered repository; a worker checks out that exact commit, reads the `.conveyor.toml` the commit carries, and executes its stages and jobs with the estate's own tools (`anvil`, `riveter`, `warehouse-cli`, plain shell). The pipeline is never stored in conveyor: only registrations, sealed secrets and credentials, and the history of runs, jobs, logs and artifacts are. Service crate `conveyor-service` 0.7.3.

## Crates

All under `services/conveyor/`, members of the Forge workspace.

| Crate | Version | Role |
|---|---|---|
| `conveyor` | 0.7.3 | The HTTP service and the workers. Library `conveyor_service`, binary `conveyor-service`. Depends on `conveyor-pipeline` (the parser), `conveyor-core`, `conveyor-web`, `forge-web`, `quench-pg`, `quench-crypto`, `quench-auth`, `quench-k8s` and the platform crates. |
| `conveyor-core` | 0.1.0 | No I/O; builds for the browser. The API's wire models (`Access`, `RunsPage`, `RunDetail`, `ScanReport`, ...), the records (`Project`, `Repo`, `Run`, `Job`, `Artifact`, `Status`, `Trigger`, `Provider`), the form drafts, the project tree, the job graph, formatting. |
| `conveyor-app` | 0.1.0 | The screens (`quench-ui`). Does no I/O: it emits `Request` values and consumes `Msg` values. |
| `conveyor-web` | 0.1.0 | The wasm build of the app over `fetch` and `EventSource`. |

The pipeline language is the separate library [conveyor-pipeline](../../libs/conveyor-pipeline/index.md); the terminal client is [conveyor-cli](../../cli/conveyor-cli/index.md).

## Service facts

* Port 9443 and base path `/conveyor` under foreman; needs gatehouse (`foreman.toml`: `needs = ["gatehouse"]`).
* Postgres is required: the module `conveyor` of foundry owns the schema (see [Data](data.md)). With an in-memory database the service starts, but the worker pool and the schedule clock are not started and every queue call answers `503`.
* Redis (or the in-process fallback) holds the realm sessions. See [Common service runtime](../common-runtime.md).
* Gatehouse: JWKS for token verification, the sign-in flow, and `POST /api/v1/notify` for run-result mail (client `conveyor-gatehouse`).
* Warehouse: where job artifacts and test reports are stored (optional).
* `git` on `PATH` for the native executor and for `git ls-remote` on a manual trigger. The service image installs it (`RUNTIME_PACKAGES = "git"`).
* A Kubernetes cluster for the `kubernetes` executor (optional).

## Authentication model

* `/api/v1/*` is wrapped by `Auth` (bearer token or session cookie, audience `conveyor`), except `/api/v1/webhooks/*`, which authenticates by HMAC signature. There is **no** blanket `RequireWrite`: each handler checks `conveyor:<action>` or `conveyor:project:<id>:<action>` itself. See [Permissions](permissions.md).
* `/ui/*` pages check the session cookie themselves and redirect to the sign-in; the SSO routes and five asset routes are public.
* With `SERVICE_AUTH_ENABLED=false` (the default of the platform) every check passes and the actor is `dev`. Registering a repository still requires the actor to exist in the realm's users table (foreign key `repos.registered_by`); a database without a `dev` user answers `400 the account making this request is not in the realm; sign in through gatehouse, which owns the estate's users`.

## Pages

| Page | Content |
|---|---|
| [API](api.md) and [openapi.yaml](openapi.yaml) | Every route with parameters, bodies and errors. |
| [Pipelines](pipelines.md) | How conveyor consumes `.conveyor.toml`: events, planning, environment, stage selection, restarts, schedules, tags. |
| [Runs and the scheduler](scheduler.md) | The queue, workers, the janitor, run/job/step status machines, cancellation, restarts, the schedule clock. |
| [Executors](executors.md) | `native`, `kubernetes` and `mock`; job services; artifact export from pods. |
| [Events](events.md) | Log streaming over SSE and the raw view. |
| [Scans](scans.md) | The six checks, where each comes from, the stored reports and the scanner findings. |
| [Permissions](permissions.md) | Grants, the project tree, what each route needs. |
| [Configuration](configuration.md) | Every environment variable. |
| [Data](data.md) | Tables, constraints, migrations. |
| [Operations](operations.md) | Running it locally and in the cluster, health, logs, failure modes. |
| [Client](client.md) | The browser client: routes, requests, messages, translations. |
