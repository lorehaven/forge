---
title: Operations
parent: Warehouse
grand_parent: Services
nav_order: 4
---

## Run locally

```bash
foreman start warehouse    # needs gatehouse (foreman starts it first); https://localhost:6443/warehouse
```

`foreman.toml`: `port = 6443`, `base_path = "/warehouse"`, `needs = ["gatehouse"]`, `GATEHOUSE_CLIENT_SECRET` and `DOCKER_TOKEN_SECRET` from the estate's shared development secrets. The browser client must be built first (`scripts/web.sh`: `services/warehouse/service/web/app.wasm`). Feature flags and storage paths are not set by foreman: set `FEATURE_*`, `FILE_STORAGES`, `DYNAMIC_STORAGE_ROOT` in `services/warehouse/service/.env` (never commit it). `service/run-debug.sh` and `generate_certs.sh` are helper scripts in the service directory.

Tests: `cargo test -p warehouse` (unit tests under `tests/unit/`, aggregated by `tests/unit.rs`); `tests/pg_storage.rs` and `tests/ui_api_pg.rs` need Postgres (`WAREHOUSE_TEST_DATABASE_URL`, module installed with `foundry-service apply --install warehouse`) and skip otherwise.

## Run in the cluster

Overlay `homecloud/overlays/forge`, directory `warehouse/`:

| Object | Notes |
|---|---|
| ConfigMap `warehouse-config` | non-secret environment: `BASE_PATH=/warehouse`, `REGISTRY_BASE_URL=https://ennor.ddns.net`, `SERVICE_NAME=warehouse`, `SERVICE_REALM=https://ennor.ddns.net/token`, `SERVICE_AUTH_ENABLED=true`, `GATEHOUSE_URL`, `GATEHOUSE_CLIENT_ID=warehouse`, all `FEATURE_*` true (`FEATURE_APK_ENABLED` is the artifact switch used), `CRATES_STORAGE_PATH=/storage/crates`, `STORAGE_PATH=/storage/docker`, `APK_STORAGE_PATH=/storage/apk`, `RIVET_STORAGE_PATH=/storage/rivets`, `FILE_STORAGES=artifacts=/artifacts;books=/books`, `DYNAMIC_STORAGE_ROOT=/mnt/extra`, `WAREHOUSE_DEFAULT_STORAGE_QUOTA_BYTES=53687091200` (50 GiB), `DB_SCHEMA=warehouse`, `LOG_SKIP_PREFIXES=/warehouse/ui,/warehouse/health` |
| Secret `warehouse-secret` | `GATEHOUSE_CLIENT_SECRET`, `DOCKER_TOKEN_SECRET`, `DATABASE_URL` (names only) |
| Deployment `warehouse` | image `ossiriand.arda:8080/forge/warehouse/warehouse:<version>`, 1 replica, `Recreate`, `depends_on` the foundry Job and `deployment/gatehouse`; requests 220m CPU / 768Mi, limits 500m CPU / 2Gi; runs as root; host-path volumes `/mnt/dev/forge/warehouse/storage` (`/storage`), `.../artifacts` (`/artifacts`), `/mnt/media/books` (`/books`, type `Directory`: the pod fails if the drive is not mounted), `/mnt/extra` (`/mnt/extra`, dynamic storage blobs on the backup drive), logs `/mnt/dev/forge/logs/warehouse` |
| Service `warehouse` | port 80 |
| Ingress `warehouse` | host `ennor.ddns.net`; paths `/warehouse/ui`, `/warehouse/index`, `/warehouse/api/v1`, `/warehouse/admin/crates`, `/warehouse/admin/docker`, `/v2`, `/token` (all Prefix to port 80); annotations `proxy-body-size: 2g`, `proxy-request-buffering: off`, read/send timeouts 600 s, forced SSL redirect, `letsencrypt-prod` |

Probes: readiness `GET /warehouse/health/ready` (initial delay 5 s, period 5 s), liveness `GET /warehouse/health/live` (initial delay 20 s, period 10 s). Readiness is withheld until `<GATEHOUSE_URL>/health/ready` answers 2xx (checked every second). Note the ingress does not route `/warehouse/health`, `/warehouse/files`-style paths outside the listed prefixes; probes bypass the ingress.

Image: `anvil docker` module `warehouse` (`images/Dockerfile.alpine`, `RESOURCE_DIR=web`, `WASM_PACKAGE=warehouse-web`), pushed to `ossiriand.arda:8080/forge` and `ennor.ddns.net/forge`.

## Health and logging

`GET /health` (`OK`), `/health/live`, `/health/ready` (under `BASE_PATH`). No metrics endpoint. Logs go to the console and to `LOG_DIR`. Startup logs which file storages are configured (`file storage \`name\` -> path`) and warns when files are enabled but `FILE_STORAGES` is empty.

## Failure modes

| Symptom | Cause |
|---|---|
| process panics at start | `GATEHOUSE_URL` unset, `DOCKER_TOKEN_SECRET` unset, no `DATABASE_URL`, session store unreachable (when a URL is set) |
| `503 gatehouse is not configured` on `/ui/login` | `GATEHOUSE_URL` or the client id/secret missing |
| every request `404` for a registry | its feature flag is off (files, artifacts, rivets) |
| files API `404` for a storage | name not in `FILE_STORAGES` and not in the database |
| dynamic upload `500 this deployment has no DYNAMIC_STORAGE_ROOT configured` | `DYNAMIC_STORAGE_ROOT` empty |
| dynamic storage calls fail with "dynamic storages need Postgres" | running with `ALLOW_IN_MEMORY_DB` |
| `429 too many concurrent upload requests` | more than `MAX_CONCURRENT_UPLOADS` Docker upload requests at once |
| `507` on upload | storage quota exceeded |
| `413` | file, artifact, rivet or Docker blob over its limit |
| `410 Gone` on `/api/v1/apk` | `FEATURE_APK_ALIAS_ENABLED=false` |

## Maintenance

`POST /admin/docker/gc` deletes blob files that no stored manifest references (manifests themselves are kept); `POST /admin/crates/gc` deletes tarballs of yanked and of unindexed versions, so a yank followed by GC is irreversible. Both need `warehouse:write` and are not scheduled by the service. Yanking never deletes files (artifacts, rivets) except through crates GC. There is no endpoint to delete an artifact or rivet version or a crate.
