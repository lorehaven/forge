---
title: Operations
parent: Gauge
grand_parent: Services
nav_order: 4
---

## Run locally

`foreman start gauge` (`foreman.toml`): builds `gauge-service`, waits for Gatehouse (`needs = ["gatehouse"]`), serves HTTPS on `0.0.0.0:13443` with `BASE_PATH=/gauge` using the certificate of `services/warehouse/service`. Open `https://localhost:13443/gauge/ui/home`. `scripts/web.sh` builds the browser module to `services/gauge/service/web/app.wasm` (foreman runs it first). See [Running the estate locally](../../guides/running-locally.md).

Without a realm (authentication off), from `services/gauge/service`, the old notes give this form (not re-run for this page; each variable is read by the platform crates, see [Common service runtime](../common-runtime.md)):

```bash
SERVICE_AUTH_ENABLED=false BASE_PATH=/gauge SERVER_ADDR=127.0.0.1:11444 SERVER_CERT_PATH=/nonexistent \
GATEHOUSE_URL=http://127.0.0.1:1 GATEHOUSE_CLIENT_ID=gauge GATEHOUSE_CLIENT_SECRET=local \
SERVICE_REALM=forge SERVICE_NAME=gauge LOG_DIR=/tmp/gauge-logs ../../target/debug/gauge-service
```

then `http://localhost:11444/gauge/ui/home`. Install the `gauge` schema first for history: `foundry-service apply --catalog services/foundry/service/migrations --database-url <url> --install gauge`.

## In the cluster

Image `gauge` built from `images/Dockerfile.alpine` with `RESOURCE_DIR=web` and `WASM_PACKAGE=gauge-web` (`.anvil/docker.toml`; registries `ossiriand.arda:8080/forge` and `ennor.ddns.net/forge`). The Dockerfile copies the service's `config` directory (so `config/targets.toml`) and the `web` directory into `/app`, and the entrypoint is `./service`. Deployment manifests are not in this repository. Needs: Gatehouse reachable (`GATEHOUSE_URL`), the three scrape targets reachable from the pod, optional Redis and Postgres. Foundry must have installed the module `gauge` into the database before history works.

## Health

`/health`, `/health/live`, `/health/ready` ([Common service runtime](../common-runtime.md)). The service is ready after Gatehouse's `/health/ready` answers 2xx. Scrape failures do not affect readiness.

## Logs

`quench_log` via `quench_starter::logging::init()`. Messages worth knowing: `Gauge service starting`, `watching N host(s) every Ns`, `<host>: scrape failed: <error>`, `history: ...` lines listed in [History](history.md).

## Failure modes

| Symptom | Cause |
|---|---|
| Panic `configuration: <path>: <error>` at start | targets file missing or invalid, or a repeated or empty target name. |
| Panic `session store` | `SessionDb::from_env` failed (Redis URL set but unreachable, or built without the redis feature). |
| Host card shows "unreachable" and an error | last scrape failed; the card keeps the last numbers dimmed. |
| History endpoints `503`, screens say history unavailable | no `DATABASE_URL`, or the first connect has not succeeded yet (retried every 30 s). |
| Warnings `history: <host>: could not store a minute` | schema `gauge` missing or wrong `DB_SCHEMA`. |
| `404` page "the browser client is not installed on this server" | `app.wasm` not in `WEB_DIR`. |
| Everything `401` | token audience is not `gauge`, or the session is not active. |

## Limits

360 points per host in memory; 8 s scrape timeout; scrape interval 1..3600 s; history retention 1..365 days; alert log at most 1000 rows per request. The history writer's channel is unbounded.

## Tests

`cargo test -p gauge-core -p gauge-app -p gauge-web -p gauge-service`. The Postgres tests in `service/tests/integration/history_tests.rs` run when `GAUGE_TEST_DATABASE_URL` is set; header of that file: start a throwaway Postgres, run `foundry-service apply ... --install gauge`, then run the tests. Unit tests: `prom_tests`, `sources_tests` (fixture `tests/fixtures/node_two_gpus.txt`), `store_tests`, `alert_log_tests`, `history_tests`. The app is tested with `TestRenderer` in `app/tests/screens.rs`; the web crate against a stand-in transport in `web/tests/unit.rs`.
