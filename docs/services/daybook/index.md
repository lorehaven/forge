---
title: Daybook
parent: Services
nav_order: 2
has_children: true
permalink: /services/daybook/
---

Daybook is a personal task manager: an inbox, custom lists, labels, due dates (with or without a time), reminders and repeat rules. One set of screens runs on Android, Linux and in the browser; the service holds the data. Service crate `daybook-service` 0.3.2.

## Crates

All live under `services/daybook/` and are members of the Forge workspace. The client crates have `publish = false`.

| Crate | Version | Role |
|---|---|---|
| `daybook-core` | 0.1.0 | Logic with no I/O: model, due dates, ordering, reminders, task form rules, calendar, widget snapshot, `Request`/`Msg` protocol. |
| `daybook-net` | 0.1.0 | The API over `gatehouse-client`; saved session and settings (`Store`). Feature `client` (default) adds the blocking client; the browser build leaves it off. |
| `daybook-host` | 0.1.0 | The `Controller` and the `Platform` trait; with feature `native` (default) the `Remote` and the worker thread. |
| `daybook-app` | 0.1.0 | The screens (quench-ui). No I/O. |
| `daybook-desktop` | 0.1.9 | Linux window (Wayland), `notify-send` reminders. Binary `daybook`. |
| `daybook-surface` | 0.1.0 | Android program, built as `libquench_surface.so`; `--widget` refreshes the widget with no window. |
| `daybook-web` | 0.1.6 | The browser build (wasm `cdylib`) over `fetch`. |
| `daybook-service` | 0.3.2 | The HTTP service: API at `/api/v1`, browser client at `/ui`, sign-in forwarding at `/auth`. |

Plus the Gradle project `android/` (package `dev.lorehaven.daybook`). See [Architecture](architecture.md).

## Service facts

* Port 12443 and base path `/daybook` under foreman; in the cluster the container listens on 80 behind the ingress path `/daybook`.
* Depends on Postgres (schema `daybook`, installed by Foundry), Redis (`REDIS_URL`, the session store) and Gatehouse (token verification, readiness, sign-in forwarding).
* Authentication: Gatehouse JWT, audience `daybook`, grants `daybook:read` and `daybook:write`; every row is scoped to the token's `sub`. See [API](api.md).
* Health: `GET {base}/health`, `/health/live`, `/health/ready` (from `quench-starter`).

## Pages

* [Architecture](architecture.md): crate layers and dependency edges.
* [API](api.md) and [openapi.yaml](openapi.yaml): the HTTP routes.
* [Configuration](configuration.md): environment, files, secrets.
* [Data](data.md): tables, migrations, local files.
* [Client](client.md): screens, the app/host protocol, platform plugins, widget.
* [Operations](operations.md): build, run, release, tests.
