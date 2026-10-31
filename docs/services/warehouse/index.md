---
title: Warehouse
parent: Services
has_children: true
permalink: /services/warehouse/
nav_order: 12
---

Warehouse is the Forge estate's storage service. One address serves a Cargo registry (JSON API and sparse index), a Docker Registry HTTP API v2, named file storages (operator-configured directories and database-backed owned storages), a multi-platform artifact registry (Android, Linux, Windows, macOS builds) and a registry of `.rivet` packages. It also hosts a browser client for browsing and managing them. Conveyor publishes build artifacts and release images to it, Gantry and `riveter` fetch rivet packages from it, and Cargo and Docker clients use it as their registry.

## Crates

| Crate | Path | Version | Role |
|---|---|---|---|
| `warehouse` | `services/warehouse/service` | 0.8.2 | the server (binary `warehouse-service`, library `warehouse_service`) |
| `warehouse-core` | `services/warehouse/core` | 0.1.0 | wire model of the management API, drafts, formatting and the file tree; shared by service and client |
| `warehouse-app` | `services/warehouse/app` | 0.1.0 | the screens (no I/O) |
| `warehouse-web` | `services/warehouse/web` | 0.1.0 | the browser module that runs the app over `fetch` |

## Where it runs

| | |
|---|---|
| Local (foreman) | package `warehouse`, port `6443`, `base_path = "/warehouse"`, `needs = ["gatehouse"]`; env `GATEHOUSE_CLIENT_SECRET`, `DOCKER_TOKEN_SECRET`. Its `cert.pem`/`key.pem` are the estate's shared development certificate |
| Cluster | `forge` overlay of `homecloud`: Deployment `warehouse`, image `ossiriand.arda:8080/forge/warehouse/warehouse:<version>`, Service port 80; see [operations](operations.md) |
| Base path | `BASE_PATH` (default `/`). The Docker routes `/v2/*` and `/token` are **always at the server root**, whatever the base path |
| Listeners | as every service: `SERVER_ADDR` (default `0.0.0.0:443`) with TLS when the certificate exists, plus the HTTP redirect on `SERVER_HTTP_REDIRECT_ADDR` (default `0.0.0.0:80`) |

## Registries at a glance

| Registry | Prefix | Enabled by | Auth | Storage |
|---|---|---|---|---|
| Crates (Cargo) | `<base>/api/v1/crates`, `<base>/index` | always | writes `warehouse:write`; reads open (or any identity with `CRATES_REQUIRE_AUTH_FOR_READS`) | `CRATES_STORAGE_PATH` (`./storage/crates`) |
| Docker / OCI | `/v2`, `/token` (root) | always | own bearer token from `/token` | `STORAGE_PATH` (`./storage/docker`) |
| Files | `<base>/api/v1/files` | `FEATURE_FILES_ENABLED` | identity to read static storages, `warehouse:write` to write them; owner/grants for dynamic storages | `FILE_STORAGES` directories; `DYNAMIC_STORAGE_ROOT` blobs + Postgres |
| Artifacts | `<base>/api/v1/artifacts` (+ `/api/v1/apk`) | `FEATURE_ARTIFACTS_ENABLED` or `FEATURE_APK_ENABLED` | identity to read, `warehouse:write` to publish/yank | `ARTIFACT_STORAGE_PATH` (`./storage/artifacts`) + Postgres `artifact_versions` |
| Rivets | `<base>/api/v1/rivets` | `FEATURE_RIVETS_ENABLED` | identity to read, `warehouse:write` to publish/yank | `RIVET_STORAGE_PATH` (`./storage/rivets`) + Postgres `rivet_packages` |
| Admin | `<base>/admin` | always | `warehouse:write` | operates on the crates and Docker storage |
| Management client | `<base>/ui`, `<base>/ui/api` | always | realm session cookie | |

## Dependencies

* **Gatehouse**: `GATEHOUSE_URL` is required (the process panics without it): token verification uses its JWKS, browser sign-in is delegated to it (authorization code with client id `GATEHOUSE_CLIENT_ID` = `warehouse`), and readiness waits for `<GATEHOUSE_URL>/health/ready`.
* **Postgres**: realm users (Docker Basic auth, owners of dynamic storages), the artifact and rivet catalogs, dynamic storages; schema `warehouse` (`DB_SCHEMA`) and the realm schema `auth` (`AUTH_DB_SCHEMA`), created by foundry modules `warehouse` and `auth`.
* **Cache store** (`REDIS_URL`/`CACHE_URL`): sessions and the JWKS cache; in-process when unset.
* The `rivet-package` library validates publishes of rivet packages.

## Pages of this section

* [API](api.md) and [openapi.yaml](openapi.yaml): all 119 routes.
* [Permissions](permissions.md): who may do what, grants and path rules.
* [Protocols](protocols.md): how Cargo, Docker, artifacts and rivets use the routes; worked examples.
* [Configuration](configuration.md)
* [Data](data.md)
* [Operations](operations.md)
* [Client](client.md)

Command-line access: the `warehouse-cli` tool (see [Command-line tools](../../cli/index.md)); library: `rivet-package` in [Libraries](../../libs/index.md).
