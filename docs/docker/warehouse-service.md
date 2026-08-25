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

Everything is under `{BASE_PATH}/api/v1/files` and behind the realm's auth — a bearer token or the realm cookie, verified against gatehouse's JWKS like any other relying party. Turned on with `FEATURE_FILES_ENABLED`. There are two kinds of storage, resolved through the same URL shape but otherwise independent of each other:

**Static storages** — the original kind, a name bound to a directory the operator configures (`FILE_STORAGES=artifacts=/storage/artifacts;media=/mnt/media`). Callers name the storage, never the directory. An unconfigured name is a `404`; a malformed `FILE_STORAGES` entry is dropped with a warning rather than treated as fatal, so one bad storage does not take the crates and Docker registries down with it. Names may use letters, digits, `-` and `_`; a duplicate name keeps the first binding. Access is the original, realm-wide model: upload/delete need the blanket `warehouse:write` grant (or a wildcard role), download/list need only a valid realm identity for this service (no separate `read` check, unchanged from before dynamic storages existed).

**Dynamic storages** — admin-provisioned, database-backed, and *owned*: created via `POST /api/v1/files` (blanket `warehouse:write` required) naming an `owner`, an optional `max_file_bytes` override, a `quota_bytes` ceiling (defaults from `WAREHOUSE_DEFAULT_STORAGE_QUOTA_BYTES`), and whether `sync_enabled`. Unlike a static storage, a dynamic one is **private by default**: only its owner, a wildcard role, or someone holding an explicit `warehouse:storage:<name>:read`/`write` grant (assigned through gatehouse's ordinary per-user permission editor, once `resource_types = ["storage"]` is declared for warehouse in `gatehouse-service`'s `config/permissions.toml`) may read or write it — the blanket `warehouse:read`/`write` grant does **not** apply here, deliberately, since the point is per-user isolation (see `routers/files/authz.rs`). `PATCH`/`DELETE /api/v1/files/{storage}` reconfigure or remove one, admin-only the same way creation is.

A dynamic storage's content is not a directory: every upload is content-addressed by SHA-256 into one shared blob store under `DYNAMIC_STORAGE_ROOT`, ref-counted so uploading identical bytes twice (the same photo backed up from two phones, or a re-run of an already-successful backup) costs a ref-count bump instead of a second copy on disk. A storage's `quota_bytes` still charges the full logical size for a dedup hit, so there's no way to game the quota by re-uploading content someone else already stored. `GET /api/v1/files/{storage}/sync?since=<id>` — available only when `sync_enabled` — returns the append-only change log (`{id, path, op, sha256, size, at}`) since cursor `since`, so a client (a phone backup app, say) can ask "what changed" instead of re-listing and re-hashing everything it already sent.

Endpoints common to both kinds: `GET /api/v1/files` (storage list — static entries by name only, dynamic entries also carry `owner`/`quota_bytes`/`used_bytes`/`sync_enabled`, filtered to what the caller may see), `GET /api/v1/files/{storage}?prefix=` (listing — a shallow directory walk for a static storage, a flat path-prefix match for a dynamic one), and `GET`/`PUT`/`HEAD`/`DELETE /api/v1/files/{storage}/file?path=`. `PUT` streams the body rather than buffering it and returns the size and SHA-256 digest; the per-file size limit is `MAX_FILE_BYTES` (default `MAX_REQUEST_BODY_BYTES`, 1GiB) for a static storage, or a dynamic storage's own `max_file_bytes` when it overrides that default.

The `path` parameter is lexically validated the same way for both kinds — `..`, absolute paths, empty paths and control bytes are refused (never normalised away). For a static storage the resolved path is separately checked to stay inside the storage root once every symlink is followed (both checks answer `403`); a dynamic storage has no filesystem path to confine in the first place, since `path` is only ever a database key resolved to a blob by digest.

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
- Postgres reachable via `DATABASE_URL` for the realm's users (Docker registry Basic auth) and, if any dynamic storage is created, for the `storages`/`blobs`/`storage_files`/`storage_sync_log` tables — schema comes from foundry's `warehouse` catalog module.
- `DOCKER_TOKEN_SECRET` set, unconditionally, or the process refuses to start.
- Redis/`REDIS_URL` for the shared session store, like every other service in the realm.
- `DYNAMIC_STORAGE_ROOT` set to a directory, if any dynamic storage is created — its content-addressed blob store lives there.

## Configuration

Selected environment variables:

| Variable | Purpose |
|---|---|
| `FEATURE_FILES_ENABLED` | turns the files API on |
| `FILE_STORAGES` | `name=path;name=path` static storage catalog |
| `MAX_FILE_BYTES` / `MAX_REQUEST_BODY_BYTES` | per-file / per-request cap for a static storage (default 1GiB) |
| `DYNAMIC_STORAGE_ROOT` | root of the shared, content-addressed blob store for dynamic storages |
| `WAREHOUSE_DEFAULT_STORAGE_QUOTA_BYTES` | quota a new dynamic storage gets when its admin doesn't name one (default 10GiB) |
| `DOCKER_TOKEN_SECRET` | signs Docker Registry bearer tokens (required, no default) |
| `SERVICE_AUTH_ENABLED` | Docker Registry auth on/off (default `false`) |
| `SERVICE_NAME` / `SERVICE_REALM` | Docker token `service`/`realm` claim (defaults `service` / `https://localhost:8698/token`) |
| `STORAGE_PATH` / `CRATES_STORAGE_PATH` | Docker / crates blob roots |
| `MAX_AUTH_FAILURES_PER_MINUTE` / `AUTH_FAILURE_WINDOW_SECONDS` | Docker Registry auth rate limit (defaults 30 / 60) |

In local dev (`foreman.toml`) warehouse runs on port 6443 under base path `/warehouse`, `needs = ["gatehouse"]`, with `GATEHOUSE_CLIENT_SECRET` and `DOCKER_TOKEN_SECRET` supplied from the estate's shared secrets.

## Testing

Unit tests live under `tests/unit/` (`files_path_tests.rs`, `files_confinement_tests.rs`, `files_storage_tests.rs`, plus the `routers_files_ops_*` and domain-level dynamic-storage tests), aggregated through `tests/unit.rs` — mostly path-safety and storage-resolution coverage for the files API. Run with `cargo test -p warehouse-service` or `foreman test warehouse` (the latter needed to exercise anything dynamic-storage-related, since that needs real Postgres).

[Home](../README.md)
