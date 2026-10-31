---
title: Operations
parent: Gatehouse
grand_parent: Services
nav_order: 4
---

## Run locally

```bash
foreman start gatehouse   # (or: foreman gatehouse) package gatehouse-service on https://localhost:5443/gatehouse
```

`foreman.toml` entry: `port = 5443`, `base_path = "/gatehouse"`, `needs = []`, `unset = ["GATEHOUSE_URL", "GATEHOUSE_CLIENT_ID"]`, `cert_from = "services/warehouse/service"`. It sets `AUTH_BOOTSTRAP=true`, `SERVICE_USERNAME`/`SERVICE_PASSWORD` (from the switchboard `.env` or the development defaults), `GATEHOUSE_KEY_ENCRYPTION_KEY`, one `CLIENT_SECRET_*` per client, `AUTH_REDIRECT_HOSTS` for the other local ports, `SERVER_HTTP_REDIRECT_ADDR=0.0.0.0:5080`, the `<NAME>_UI_URL` of every local service, and (shared by all services) `REDIS_URL`, `RUST_LOG`, `DATABASE_URL` for foreman's local Postgres container. The browser client must be built first (`scripts/web.sh` builds every `services/*/web` crate to `services/<name>/service/web/app.wasm`; foreman runs it before a start). Without `app.wasm` the page routes answer `404` with `the browser client is not installed on this server`.

Tests: `cargo test -p gatehouse` (unit tests in `tests/unit/*.rs`, aggregated by `tests/unit.rs`; `tests/pg_models.rs` needs a Postgres reachable through `GATEHOUSE_TEST_DATABASE_URL` with the `gatehouse` module installed by `foundry-service apply --install gatehouse`, and skips itself otherwise). `foreman test` runs the workspace suites with the local infrastructure (flags are passed through). The BDD features are in `tests/forge-bdd/features/gatehouse/`.

## Run in the cluster

Overlay `homecloud/overlays/forge` (package `forge`), directory `gatehouse/`:

| Object | File | Notes |
|---|---|---|
| ConfigMap `gatehouse-config` | `config.yaml.j2` | non-secret environment (below) |
| Secret `gatehouse-secret` | `secret.yaml.j2` | `GATEHOUSE_KEY_ENCRYPTION_KEY`, `CLIENT_SECRET_*` (names only) |
| ConfigMap `gatehouse-clients-config` | `clients-configmap.yaml.j2` | mounted at `/config`; `CLIENTS_CONFIG=/config/clients.toml` |
| ConfigMap `gatehouse-permissions-config` | `permissions-configmap.yaml.j2` | mounted at `/config-permissions`; `PERMISSIONS_CONFIG=/config-permissions/permissions.toml` |
| Deployment `gatehouse` | `deployment.yaml.j2` | image `ossiriand.arda:8080/forge/gatehouse/gatehouse:<version>`, `Always` pull, 1 replica, strategy `Recreate`, `depends_on` the foundry Job and `deployment/redis`, service account `gatehouse-sa`, requests 100m CPU / 256Mi, limits 300m / 512Mi, runs as uid/gid 0, host-path logs `/mnt/dev/forge/logs/gatehouse` at `/logs` |
| Service `gatehouse` | `service.yaml.j2` | port 80 |
| Ingress `gatehouse` | `ingress.yaml.j2` | host `ennor.ddns.net`, path `/gatehouse` (Prefix), `letsencrypt-prod`, forced SSL redirect, proxy timeouts 60 s |

Probes: readiness `GET /gatehouse/health/ready` (initial delay 3 s, period 5 s), liveness `GET /gatehouse/health/live` (initial delay 15 s, period 10 s), both port 80. Values from `overlays/forge/values.yaml`: `SERVICE_AUTH_ENABLED=true`, `ACCESS_TOKEN_TTL_SECS=900`, `REFRESH_TOKEN_TTL_SECS=2592000` (30 days), `AUTH_REDIRECT_HOSTS=https://ennor.ddns.net`, `LOG_SKIP_PREFIXES=/gatehouse/ui,/gatehouse/health`, SMTP through the in-cluster Stalwart (`465`, `implicit`, certificate name `ennor.ddns.net`), `MAIL_FROM=noreply@ennor.ddns.net`. While Gatehouse is down existing sessions keep working at the relying parties (they read the cache store) but nobody can log in or refresh.

Image: built by `anvil docker` module `gatehouse` (`images/Dockerfile.alpine`, build args `RESOURCE_DIR=web`, `WASM_PACKAGE=gatehouse-web`); pushed to `ossiriand.arda:8080/forge` and `ennor.ddns.net/forge`.

Deploy order: the foundry Job (`auth` and `gatehouse` modules) must finish before a Gatehouse that reads a new table starts; Redis must exist.

## Health and monitoring

* `GET /health` always `200 OK`; `GET /health/live` and `/health/ready` report `HealthState`. Gatehouse marks itself ready after start-up (`health_state.mark_ready()` once the database, keys and stores are initialised).
* No Prometheus endpoint exists in this service.
* Logging: `quench-log` to the console, and to `LOG_DIR`/`LOG_FILE_PREFIX` files when set. Login attempts, refusals, admin actions (`created user`, `updated user`, `deleted user`, `disabled`/`enabled`, `unlocked`, MFA changes) and mail results are logged at info/warn; passwords and tokens are not.

## Rate limits and the mail budget

Fixed windows counted in the cache store (shared by replicas; a store error counts as limited):

| Scope | Limit | Subject |
|---|---|---|
| registration | 20 per hour; 3 per day | client address; email address |
| password reset | 20 per hour; 3 per hour | client address; submitted username |
| resend verification | 20 per hour; 3 per hour; 1 per minute | client address; username; username |
| email change | 3 per hour; 20 per hour; 3 per day | account; client address; target address |
| notify | 20 per hour; 10 per hour | person; person and template |
| all mail | `MAIL_DAILY_LIMIT` per UTC day (default 250) | whole estate |

The client address is `X-Real-IP`, else the last entry of `X-Forwarded-For` if it parses as an IP address, else the shared bucket `unknown`. It is only as trustworthy as the proxy that sets the headers (ingress-nginx with `externalTrafficPolicy: Local` overwrites them); every per-address limit is therefore backed by a per-account or per-address one. Limits are not configurable.

## Failure modes

| Symptom | Cause | Effect |
|---|---|---|
| process panics at start | missing `GATEHOUSE_KEY_ENCRYPTION_KEY`, `SERVICE_USERNAME`/`SERVICE_PASSWORD` with bootstrap on, unreadable `permissions.toml`, no `DATABASE_URL`, invalid mail settings, cache store unreachable (when a URL is set), stored signing key undecryptable | no listener |
| `clients.toml` unreadable | logged as an error | no client is seeded; authorization-code and `client_credentials` grants reject every client |
| client without secret variable | warning, skipped | that client cannot get tokens |
| no `SMTP_HOST` | warning at start | links are logged instead of mailed |
| mail budget used up | sends fail with a transient error | pages answer as usual; notify answers `502 failed` with `retryable: true` |
| cache store down at run time | rate limiter says "limited"; session calls fail | logins fail (`500` or error redirect) |
| `app.wasm` missing | page routes `404` | JSON API unaffected |

## Limits

Request bodies are limited to 2 MiB by the framework (`BodyLimit`), avatar uploads to 262 144 bytes, a username to 64 characters, notification variables to 300 characters (links 500), application tokens to 365 days.

## Security notes

The browser-facing public routes (no session) are listed in `tests/forge-sectest/public-routes.txt` for the nightly scan: `/`, `/.well-known/jwks.json`, `/health*`, `/swagger-ui*`, `/ui/login`, `/ui/register`, `/ui/forgot-password`, `/ui/resend-verification` (and their slash forms) and `/ui/status`. Other anonymous routes by design: `/ui/assets/*`, `/ui/verify`, `/ui/reset-password`, `/ui/accept-invite`, `/ui/confirm-email`, `/ui/unsubscribe`, `/api/v1/auth/*` (credential in body), `/api/v1/token`, `/api/v1/authorize`.
