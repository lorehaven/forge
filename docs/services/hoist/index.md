---
title: Hoist
parent: Services
nav_order: 8
has_children: true
---

Hoist is the app store for the estate: a native app for Android and Linux that lists the builds in Warehouse's artifact registry, installs and updates them, and (on Android) also browses a public catalogue of F-Droid, IzzyOnDroid and Aptoide apps. It has **no browser client**. Its only server-side part is `hoist-service`, the public catalogue.

## Crates

All crates live under `services/hoist/` and are members of the Forge workspace.

| Crate | Version | Role |
|---|---|---|
| `hoist-core` | 0.1.0 | Logic with no I/O: registry rows, the install-state join, public catalogue shapes, version codes. |
| `hoist-net` | 0.1.0 | Blocking calls to Gatehouse (through `gatehouse-client`) and Warehouse; downloads with SHA-256 check. |
| `hoist-app` | 0.1.0 | The screens (`quench-ui`); `crate-type = ["cdylib", "rlib"]`. |
| `hoist-desktop` | 0.1.7 | The Linux program `hoist`: Wayland window, worker thread, `--check`. Its version is the app's version (`.anvil/hoist.toml`: `version_file = "desktop/Cargo.toml"`). |
| `hoist-surface` | 0.1.0 | The Android program (binary `hoist_surface`, shipped as `libquench_surface.so`): the same screens drawn by the native renderer, and `--check`. |
| `hoist-service` | 0.1.1 | The public catalogue HTTP service (library `hoist_service`, binary `hoist-service`). |
| `android/` | - | Gradle project (`dev.lorehaven.hoist`): host, `packages` plugin, update job. |

Package id (Android application id, Linux id, registry program): `dev.lorehaven.hoist`.

## Dependencies

* `hoist-service`: Postgres (schema `hoist`, installed by Foundry's `hoist` module), Redis (the realm's session store), Gatehouse (tokens, readiness). It also fetches three upstream stores over HTTPS.
* The apps: Gatehouse (`/gatehouse`, sign-in), Warehouse (`/warehouse/api/v1/artifacts`), `hoist-service` (`/hoist/v1/...`), all behind one server address (default `https://ennor.ddns.net`).

## Authentication

* `hoist-service`: every route under `{BASE_PATH}/v1` requires a valid realm token (`quench-auth` `Auth` middleware, scoped by path prefix). Any identity passes; there is no permission check and nothing is per user. Health probes are outside `/v1` and are open.
* The apps sign in with user name and password against Gatehouse once and keep only the refresh token. Accounts with MFA cannot sign in (the app cannot ask for a second factor).

## Pages

* [Architecture](architecture.md): layers and data flow.
* [API](api.md) and [openapi.yaml](openapi.yaml): the catalogue service.
* [Configuration](configuration.md): service variables, app files and settings.
* [Data](data.md): the Postgres table and the app's files.
* [Client](client.md): screens, requests and messages, platform plugins.
* [Operations](operations.md): build, run, release.
