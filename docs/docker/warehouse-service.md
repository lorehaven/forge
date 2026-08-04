# Warehouse Service

Warehouse is the Forge estate's storage service: one address for a Cargo registry, a Docker Registry HTTP API v2, and plain file storage, mounted side by side because storage is storage even when the addressing scheme differs — a crate by name and version, an image by digest, a file by path within a named directory. Conveyor is the first real caller of the files API (build artifacts) and of the Docker registry (release images); the crates registry lets the estate host its own internal Rust crates. Binary and crate name: `warehouse-service` (`docker/warehouse-service`).

## Features

- **Files API** — plain named-storage file upload/download/list/delete, streamed to disk rather than buffered, behind the realm's normal bearer/cookie auth.
- **Docker Registry HTTP API v2** — the standard blob/manifest/catalog routes, with its own Basic-then-Bearer token exchange independent of the realm's JWKS-based tokens.
- **Crates registry** (`/api/v1/crates`) — publish, download, yank/unyank, owners and search, plus a sparse index. Has no real authentication yet.
- **Admin endpoints** (`/admin`) — garbage collection for both the crates and Docker storages.
- **A small server-rendered UI** for browsing the crates and Docker catalogs.

## Architecture

Two mount points, split by where the protocol forces them to live (`src/lib.rs`):

- **`root_scope`** — mounted at the server root, outside `BASE_PATH`. Holds `/v2/*` and `/token`, because the Docker Registry spec fixes those paths; wrapped in `WarehouseAuth` (the registry's own Bearer-token middleware, with a per-client auth-failure rate limit — `MAX_AUTH_FAILURES_PER_MINUTE`, default 30, `AUTH_FAILURE_WINDOW_SECONDS`, default 60) and `WarehouseLimits` (concurrent-upload cap).
- **`base_path_scope`** — everything else: `/admin`, `/api/v1/crates` (+ the sparse index), `/api/v1/files`, and the UI.

### Files API

Everything is under `{BASE_PATH}/api/v1/files` and behind the realm's auth — a bearer token or the realm cookie, verified against gatehouse's JWKS like any other relying party. Turned on with `FEATURE_FILES_ENABLED`; upload and delete need `warehouse:write` (or a wildcard role), download and list need only `warehouse:read`, enforced by `quench-auth`'s `RequireWrite`.

A *storage* is a name bound to a directory (`FILE_STORAGES=artifacts=/storage/artifacts;media=/mnt/media`). Callers name the storage, never the directory. An unconfigured name is a `404`; a malformed entry is dropped with a warning rather than treated as fatal, so one bad storage does not take the crates and Docker registries down with it. Names may use letters, digits, `-` and `_`; a duplicate name keeps the first binding.

Endpoints: `GET /api/v1/files` (storage names), `GET /api/v1/files/{storage}?prefix=` (shallow listing), and `GET`/`PUT`/`HEAD`/`DELETE /api/v1/files/{storage}/file?path=`. `PUT` streams the body straight to a sibling of the target and renames it into place, returning the size and SHA-256 digest; `MAX_FILE_BYTES` (default `MAX_REQUEST_BODY_BYTES`, 1GiB) is enforced as bytes arrive rather than after buffering the whole thing.

The `path` parameter is the entire attack surface: `..`, absolute paths, empty paths and control bytes are refused lexically (never normalised away — a symlink component would make normalisation wrong anyway), and the resolved path is separately checked to stay inside the storage root once every symlink is followed. Both checks answer `403`.

The crates registry and `/admin` are **not** behind any of this — they have no real authentication yet, so there is no identity to build a permission on top of. That is separate, unstarted work.

### Docker Registry API v2

Auth flow: a client hits `/v2/...`; gets `401` with `WWW-Authenticate: Bearer realm="<SERVICE_REALM>",service="<SERVICE_NAME>"` if its token is missing or invalid; requests `GET /token?service=&scope=repository:<repo>:pull,push` with HTTP Basic (validated against the realm's users via `DATABASE_URL` — the same store gatehouse writes to, not a local username/password pair), or anonymously when `SERVICE_AUTH_ENABLED=false`; then retries `/v2/...` with `Authorization: Bearer <jwt>`.

`DOCKER_TOKEN_SECRET` signs and verifies these tokens — local to warehouse, entirely independent of the realm's Ed25519 keys, since the registry protocol speaks its own exchange gatehouse has no part in. It is **always** required: the service panics at startup without it, regardless of whether `SERVICE_AUTH_ENABLED` is even on. `SERVICE_NAME`, `SERVICE_REALM` and `SERVICE_AUTH_ENABLED` are the same env vars `quench-auth`'s `JwtConfig` reads for the realm token, reused here for the Docker realm string.

| Method | Path | Purpose |
|---|---|---|
| `GET`/`HEAD` | `/v2/` | registry availability check |
| `GET` | `/v2/_catalog?n=&last=` | paginated repository list |
| `GET` | `/v2/{name}/tags/list?n=&last=` | paginated tags for a repository |
| `POST` | `/v2/{name}/blobs/uploads/` | start an upload (or cross-repo mount via `mount`+`from`) |
| `GET` | `/v2/{name}/blobs/uploads/{uuid}` | upload status |
| `PATCH` | `/v2/{name}/blobs/uploads/{uuid}` | upload a chunk |
| `PUT` | `/v2/{name}/blobs/uploads/{uuid}?digest=sha256:...` | complete an upload |
| `DELETE` | `/v2/{name}/blobs/uploads/{uuid}` | cancel an upload |
| `HEAD` | `/v2/{name}/blobs/{digest}` | check a blob exists |
| `GET` | `/v2/{name}/blobs/{digest}` | retrieve a blob (`Range` supported) |
| `PUT` | `/v2/{name}/manifests/{reference}` | upload a manifest (Docker/OCI media types) |
| `GET` | `/v2/{name}/manifests/{reference}` | fetch a manifest by tag or digest |
| `HEAD` | `/v2/{name}/manifests/{reference}` | check a manifest exists |
| `DELETE` | `/v2/{name}/manifests/{reference}` | delete a manifest (digest reference required) |
| `GET` | `/token?service=&scope=` | issue a JWT for Docker Bearer auth |

### Crates registry and admin

`/api/v1/crates` implements the Cargo registry protocol (publish, download, yank/unyank, owners, search) plus a sparse index under `/api/v1/crates/index`. `/admin` exposes garbage collection for both the crates and Docker blob storages. Neither surface authenticates callers today.

## Requirements

- A relying party of gatehouse: `GATEHOUSE_URL`/OAuth client config for realm sessions on the files API and the UI.
- Postgres reachable via `DATABASE_URL` for the realm's users (Docker registry Basic auth) — schema comes from foundry's `warehouse` catalog module.
- `DOCKER_TOKEN_SECRET` set, unconditionally, or the process refuses to start.
- Redis/`REDIS_URL` for the shared session store, like every other service in the realm.

## Configuration

Selected environment variables:

| Variable | Purpose |
|---|---|
| `FEATURE_FILES_ENABLED` | turns the files API on |
| `FILE_STORAGES` | `name=path;name=path` storage catalog |
| `MAX_FILE_BYTES` / `MAX_REQUEST_BODY_BYTES` | per-file / per-request cap (default 1GiB) |
| `DOCKER_TOKEN_SECRET` | signs Docker Registry bearer tokens (required, no default) |
| `SERVICE_AUTH_ENABLED` | Docker Registry auth on/off (default `false`) |
| `SERVICE_NAME` / `SERVICE_REALM` | Docker token `service`/`realm` claim (defaults `service` / `https://localhost:8698/token`) |
| `STORAGE_PATH` / `CRATES_STORAGE_PATH` | Docker / crates blob roots |
| `MAX_AUTH_FAILURES_PER_MINUTE` / `AUTH_FAILURE_WINDOW_SECONDS` | Docker Registry auth rate limit (defaults 30 / 60) |

In local dev (`foreman.toml`) warehouse runs on port 6443 under base path `/warehouse`, `needs = ["gatehouse"]`, with `GATEHOUSE_CLIENT_SECRET` and `DOCKER_TOKEN_SECRET` supplied from the estate's shared secrets.

## Testing

Unit tests live under `tests/unit/` (`files_path_tests.rs`, `files_confinement_tests.rs`, `files_storage_tests.rs`), aggregated through `tests/unit.rs` — mostly path-safety and storage-resolution coverage for the files API. Run with `cargo test -p warehouse-service` or `foreman test warehouse`.

[Home](../README.md)
