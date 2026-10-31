---
title: Operations
parent: Conveyor
grand_parent: Services
nav_order: 4
---

Run, deploy, observe and test conveyor. Nothing here contacts a real service; commands that do are described, not run.

## Run it locally

Under foreman (`foreman.toml`: package `conveyor`, port 9443, base path `/conveyor`, needs gatehouse, the estate's dev certificate from `certs/`, `GATEHOUSE_CLIENT_SECRET` from `client_secret_conveyor`):

```bash
foreman start conveyor      # builds conveyor-service, starts postgres and redis, installs the schema, starts gatehouse and conveyor
foreman logs conveyor
foreman stop conveyor
```

The seed (`testdata/*.sql`) loads demo repositories, runs and jobs for conveyor. See [Running the estate locally](../../guides/running-locally.md). The UI is `https://localhost:9443/conveyor/ui/home` (`CONVEYOR_UI_URL` of gatehouse's catalog).

Without gatehouse (API experiments only), the platform defaults allow a bare run: `SERVICE_AUTH_ENABLED=false` (the default), `DATABASE_URL` for a Postgres whose `conveyor` and `auth` schemas were installed by foundry (and a `dev` row in `auth.users` if you register repositories, because `repos.registered_by` is a foreign key), `GATEHOUSE_URL` (the JWKS verifier is built at start and panics without it), `GATEHOUSE_CLIENT_ID`/`GATEHOUSE_CLIENT_SECRET`, `WEB_DIR` with `app.wasm`. A minimal webhook round trip against it needs `CONVEYOR_WEBHOOK_SECRET`.

The [conveyor-cli](../../cli/conveyor-cli/index.md) tool wraps these calls; the calls themselves are on the [API](api.md) page.

## In the cluster

* Image: `anvil docker` module `conveyor` (`.anvil/docker.toml`): image name `conveyor`, registries `ossiriand.arda:8080/forge` and `ennor.ddns.net/forge`, build arguments `RUNTIME_PACKAGES=git`, `RESOURCE_DIR=web`, `WASM_PACKAGE=conveyor-web`. The container listens on 80 behind the ingress path `/conveyor`.
* The estate overlay (`homecloud`, package `forge`, directory `conveyor/`): ConfigMap `conveyor-config`, Secret `conveyor-secret`, Deployment (`replicas: 1`, `strategy: Recreate`), Service, Ingress, and `role.yaml.j2` (the Role that lets conveyor's service account manage Jobs, pods, pod logs and Secrets for the `kubernetes` executor). Names and values are in that repository, not here.
* Probes: readiness `GET /conveyor/health/ready`, liveness `GET /conveyor/health/live`.
* Replicas: several replicas can share one database; the claim uses `FOR UPDATE SKIP LOCKED` and the per-repository index. Live log streaming only works against the replica running the job; a load balancer in front must therefore keep a viewer on one replica or accept that the stream shows the stored log until the job ends.
* The native executor needs the toolchain in the image (`cargo`, `docker`, `kubectl`, `anvil`, `riveter` are only reported as missing at start; pipelines that use them fail otherwise). The image `rust-builder` (`images/rust-builder`, tag `ossiriand.arda:8080/forge/ci/rust-builder:latest`) is what the Forge pipeline's jobs name as their `image`, which only the `kubernetes` executor honours.

## Health and observability

| Endpoint | Answer |
|---|---|
| `GET /health` | `200 OK` text. |
| `GET /health/live` | `200 {"status":"live"}`. |
| `GET /health/ready` | `503 {"status":"not_ready"}` until gatehouse's `/health/ready` answers (`wait_for_services("conveyor-service", [gatehouse])` polls once a second), then `200 {"status":"ready"}`. It does not check Postgres or the executor. |

These come from `quench-starter` and are not in `openapi.yaml` (the checker reads only the service's own sources). `GET /swagger-ui` and the redirects of `BASE_PATH` are described in [Common service runtime](../common-runtime.md).

There is no metrics endpoint.

Logging: `quench-log` (`RUST_LOG`, `LOG_DIR`, daily files `app.<date>.log`). Notable lines: `conveyor configured: executor <x>, work dir <path>, <n> concurrent run(s)`; `scheduler started: <n> worker(s), <x> executor`; `run <id> claimed: <ref> at <sha7>`; `run <id> finished: <status>`; `job <name> finished: <status>`; `requeued <n> run(s) abandoned by a dead worker`; `schedule `<expr>` (<tz>) of <slug> came due: run started`; `kept <path> (<n> bytes) as <remote>`; `delivery ignored: <reason>` (debug); `rejected a <provider> delivery: bad signature` (warn); `git found; checkouts will work` or `git is not on PATH: every run will fail at the checkout ...`.

## Failure modes

| Symptom | Cause and behaviour |
|---|---|
| Every queue call `503 conveyor's queue needs Postgres ...` | the database is in-memory (`ALLOW_IN_MEMORY_DB`); no workers or clock were started (`scheduler not started`). |
| Runs stay `queued` | no worker (in-memory DB), or the repository already has a `running` run, or the repository is disabled. |
| A run is `running` for a long time after a restart | its claim is stale for up to `CONVEYOR_CLAIM_STALE_AFTER_SECS`; then the janitor requeues it. |
| Run `failed`: `checkout failed: git is not on PATH ...` | git missing on a native deployment. |
| Run `failed`: `this commit has no .conveyor.toml` | see [Pipelines](pipelines.md). |
| Job `failed`: `this pipeline needs secrets that are not set: X` | the secret is neither a repository nor an estate secret, or `CONVEYOR_SECRET_KEY` is unset. |
| Secret writes `503` | `CONVEYOR_SECRET_KEY` unset or malformed. |
| Webhooks `503 conveyor is not configured to accept webhooks for this repository` | no repository `WEBHOOK_SECRET` and no `CONVEYOR_WEBHOOK_SECRET`. Deliveries are never accepted unverified. |
| Webhook `202 {"ignored": ...}` and no run | disabled repository, fork PR, tag push, or an event with nothing to build; the reason is in the body. |
| `kubernetes` jobs all fail immediately | the cluster was unreachable at start (`kubernetes executor cannot run anything ...`); restart after fixing. |
| Commit status never appears on GitHub | `CONVEYOR_GITHUB_TOKEN` unset (silent), or GitHub rejected it (warning `could not report ...`). |
| No scan cards for tests/security | `WAREHOUSE_URL` unset, or no job left a `quench-test` report. |
| Failure mail not sent | `CONVEYOR_PUBLIC_URL`, `GATEHOUSE_URL` or `CLIENT_SECRET_CONVEYOR_GATEHOUSE` unset; nobody follows; or rows in `notification_outbox` with `failed_at` set (`last_error`). |
| A job's live log is empty on the page though it runs | the job runs on another replica (only that replica holds live output). |

## Limits

* Concurrency: `CONVEYOR_MAX_CONCURRENT_RUNS` runs per replica; one running run per repository across all replicas.
* Per job: log channel of 1024 lines (slow subscribers get `lagged`); 24 MiB per file handed back from a pod; 512 MiB per declared artifact; reports downloaded for scans at most 16 MiB.
* Scan: 50 findings per check, newest 40 runs searched, newest 30 for the stage list, 200 runs read for the front page.
* `GET /runs` `limit` at most 500.
* Stage selection: 64 names of at most 128 characters.

## Tests

`services/conveyor/service/tests/`: `unit` (no database: config, status machine, executors including the manifest builder and the native executor with real processes, providers, secret sealing and redaction, scan parsing, the SSE frames, the API's authorisation) and `integration` (Postgres: queue, scheduler, worker with the mock executor, schedules, routers, notifications, webhooks). Both use the custom harness (`harness = false`). The integration suite is skipped unless `CONVEYOR_TEST_DATABASE_URL` is set to a throwaway Postgres (never the foreman container):

```bash
CONVEYOR_TEST_DATABASE_URL=postgres://postgres:postgres@localhost:55432/postgres cargo test -p conveyor --test integration
cargo test -p conveyor --test unit
cargo test -p conveyor-core -p conveyor-app -p conveyor-web
```

The BDD suite `forge-bdd` (`@conveyor`, port 9999) covers route authentication and validation without a database. See [Testing](../../guides/testing.md).
