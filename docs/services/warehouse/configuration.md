---
title: Configuration
parent: Warehouse
grand_parent: Services
nav_order: 2
---

Environment variables read by `warehouse-service` and the framework crates it uses. Several numeric settings are read through `quench_config::ConfigLoader::new("WAREHOUSE")`: such a variable can be set as `WAREHOUSE_<NAME>` or plain `<NAME>` (the prefixed form wins); the table says which.

## Server and process

| Variable | Default | Meaning |
|---|---|---|
| `BASE_PATH` | `/` | prefix for everything except `/v2` and `/token` |
| `SERVER_ADDR` | `0.0.0.0:443` | listen address; plain HTTP here when no certificate |
| `SERVER_HTTP_REDIRECT_ADDR` | `0.0.0.0:80` | HTTP to HTTPS redirect listener |
| `SERVER_CERT_PATH`, `SERVER_KEY_PATH` | `cert.pem`, `key.pem` | TLS files |
| `RUST_LOG` | `info` | log level |
| `LOG_DIR`, `LOG_FILE_PREFIX` | | log files |
| `LOG_SKIP_PREFIXES` | empty | comma-separated path prefixes the request logger skips |

## Realm and Gatehouse

| Variable | Default | Meaning |
|---|---|---|
| `GATEHOUSE_URL` | **required** | Gatehouse including its base path; JWKS, login redirect, token exchange, readiness probe |
| `GATEHOUSE_CLIENT_ID` | | OAuth client id (`warehouse`) |
| `GATEHOUSE_CLIENT_SECRET` | | its secret; without id and secret the login redirect is `503` |
| `GATEHOUSE_TLS_VERIFY` | `true` | `false` accepts invalid certificates for JWKS and token calls (development) |
| `SERVICE_NAME` | `service` | **must be `warehouse`**: the token audience this service checks, the Docker token `service` |
| `SERVICE_REALM` | `https://localhost:8698/token` | the `realm` in Docker `WWW-Authenticate` challenges (set to the public `/token` URL) |
| `SERVICE_AUTH_ENABLED` | `false` | `true` turns every realm check on; false bypasses them (development only) |
| `AUTH_DB_SCHEMA` | `auth` | realm schema (users) |
| `AUTH_COOKIE_NAME`, `AUTH_REFRESH_COOKIE_NAME`, `AUTH_COOKIE_DOMAIN` | `forge_session`, `forge_refresh`, host-only | cookie names/scope; must match Gatehouse |
| `ACCESS_TOKEN_TTL_SECS`, `REFRESH_TOKEN_TTL_SECS` | `900`, `604800` | read by `JwtConfig`; the refresh value is the refresh cookie's Max-Age |
| `REDIS_URL` / `CACHE_URL` | none (in-process) | session and JWKS cache store |
| `DOCKER_TOKEN_SECRET` | **required** | HS256 secret of Docker registry tokens (the process panics without it, even with auth disabled) |
| `MAX_AUTH_FAILURES_PER_MINUTE` | `30` | failed registry authentications per client within the window before `429` |
| `AUTH_FAILURE_WINDOW_SECONDS` | `60` | the window |

## Database

| Variable | Default | Meaning |
|---|---|---|
| `DATABASE_URL` (or `POSTGRES_URL`) | **required** | Postgres; empty stops the process unless `ALLOW_IN_MEMORY_DB=true` (tests) |
| `DB_SCHEMA` | `warehouse` | schema of the service's own tables |
| `DB_POOL_MAX_SIZE` | `5` | pool size |

Dynamic storages require Postgres (`StorageError::NotPostgres` otherwise).

## Feature flags

| Variable | Default | Effect |
|---|---|---|
| `FEATURE_FILES_ENABLED` | off | files API and its UI pieces; read once at first use |
| `FEATURE_ARTIFACTS_ENABLED` (or `FEATURE_APK_ENABLED`) | off | artifact API (and the `/api/v1/apk` alias); read once |
| `FEATURE_RIVETS_ENABLED` | off | rivet API; read on every call |
| `FEATURE_DOCKER_ENABLED`, `FEATURE_CRATES_ENABLED` | off | **only** what `/ui/api/overview` reports; the routes are always served |
| `FEATURE_APK_ALIAS_ENABLED` | `true` | `false` answers all of `/api/v1/apk` with `410 Gone` (read on every call) |

Truthy values: `1`, `true`, `yes`, `on`; falsy: `0`, `false`, `no`, `off` (case-insensitive); anything else means the default.

## Storage

| Variable | Default | Meaning |
|---|---|---|
| `STORAGE_PATH` | `./storage/docker` | Docker blobs, manifests, tags, uploads |
| `CRATES_STORAGE_PATH` | `./storage/crates` | crate tarballs, owners, index |
| `ARTIFACT_STORAGE_PATH` (fallback `APK_STORAGE_PATH`) | `./storage/artifacts` | artifacts; at start, a legacy `APK_STORAGE_PATH` (default `./storage/apk`) tree `<program>/<code>/` is moved to `<program>/android/<code>/` under the artifact root (best effort) |
| `RIVET_STORAGE_PATH` | `./storage/rivets` | rivet packages |
| `FILE_STORAGES` | empty | static storages, `name=path;name=path`; a malformed, duplicate or badly named entry is dropped with a warning |
| `DYNAMIC_STORAGE_ROOT` | empty | root of the content-addressed blob store (`.blobs/`, `.tmp/`); required to create or use dynamic storage uploads |
| `DEFAULT_STORAGE_QUOTA_BYTES` (`WAREHOUSE_` prefix works) | `10737418240` | quota of a new dynamic storage |

## Limits

| Variable | Default | Meaning |
|---|---|---|
| `MAX_FILE_BYTES` (`WAREHOUSE_` prefix works) | `MAX_REQUEST_BODY_BYTES` | largest file in the files API and artifact publish |
| `MAX_REQUEST_BODY_BYTES` (`WAREHOUSE_` prefix works) | `1073741824` | default for the above |
| `MAX_DOCKER_BLOB_BYTES` (`WAREHOUSE_` prefix works) | `34359738368` (32 GiB) | assembled Docker blob |
| `MAX_CONCURRENT_UPLOADS` (`WAREHOUSE_` prefix works) | `32` | concurrent Docker upload requests (POST/PATCH/PUT on `/blobs/uploads`) |
| `RIVET_MAX_BYTES` | `67108864` | one compressed `.rivet` |
| Framework body limit | 2 MiB | buffered JSON/form/text bodies (not the streamed uploads) |

## Registry URLs

| Variable | Default | Meaning |
|---|---|---|
| `REGISTRY_BASE_URL` | `https://localhost` | public origin put in the sparse index `config.json` (`dl`, `api`) |
| `CRATES_REQUIRE_AUTH_FOR_READS` | `false` | require a realm identity for crates reads and advertise `auth-required` |
| `ENABLE_REDIRECT` | `false` | answer Docker blob GETs with 307 to `BLOB_REDIRECT_BASE` |
| `BLOB_REDIRECT_BASE` | `https://storage.example.com` | redirect target base (`/blobs/sha256/<hex>` is appended) |
| `APK_ALIAS_SUNSET` | unset | HTTP-date for the `Sunset` header of the alias |

## Secrets (names only)

`GATEHOUSE_CLIENT_SECRET`, `DOCKER_TOKEN_SECRET`, `DATABASE_URL` (cluster Secret `warehouse-secret`). `service/.env` holds local values; never commit it.

## Files in the repository

`service/migrations/00001-initial-users-schema.toml` and `00002-auth-sessions.toml` create a per-service `warehouse.users`/`warehouse.sessions` schema from before the shared realm; they are not part of the foundry catalog (`services/foundry/service/migrations/warehouse`) that deployments use. `service/i18n/<locale>.ftl` (260 lines each, five locales) are compiled into `warehouse-app` (see [client](client.md)). `service/dist/assets` is a leftover of the earlier server-rendered UI; no route reads it.
