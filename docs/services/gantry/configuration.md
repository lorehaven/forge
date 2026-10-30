---
title: Configuration
parent: Gantry
grand_parent: Services
nav_order: 2
---

Every environment variable `gantry-service` and `gantry-runner` read. `Settings::from_env` reads the process environment with `std::env::var`; **an empty value counts as unset**. Variables of the platform crates are listed once in [Common service runtime](../common-runtime.md). Secrets are named, never valued.

## Modes (the two that matter)

| Variable | Values | Default | Meaning |
|---|---|---|---|
| `GANTRY_CLUSTER` | `in-cluster`, `kubeconfig`, `none` | `in-cluster` when `KUBERNETES_SERVICE_HOST` is set (a pod), else `none` | How the cluster is read. `none` is an in-memory fake. Any other value stops the start: `GANTRY_CLUSTER '<x>' is not in-cluster, kubeconfig or none`. |
| `GANTRY_RUNNER` | `job`, `local`, `dry-run`, `simulate` | `job` when the cluster is `in-cluster`, else `dry-run` | How operations run. Any other value stops the start. |

Combinations that are refused at start (the process panics with `configuration: <reason>`):

| Pair | Reason |
|---|---|
| `job` + `none` | needs a cluster to put the Job in |
| `simulate` + `kubeconfig` or `in-cluster` | simulate changes the fake cluster only; against a real one it would show changes that never happened |
| `local` + `none` or `in-cluster` | local runs `kubectl` as the developer, so it needs `GANTRY_CLUSTER=kubeconfig`, never the fake cluster |

Therefore writing to a real cluster from a developer machine has to be said twice: `GANTRY_CLUSTER=kubeconfig` **and** `GANTRY_RUNNER=local`. A pod defaults to `in-cluster` + `job`. With nothing set: `none` + `dry-run`.

## Cluster and runner

| Variable | Default | Meaning |
|---|---|---|
| `GANTRY_KUBE_CONTEXT` | none | Pins a kubeconfig context (only with `kubeconfig`). |
| `GANTRY_NAMESPACE` | the pod's namespace file, else `forge` | Where runner Jobs go. |
| `GANTRY_NAMESPACES` | empty | Comma list. Allow-list: planning or running anything outside it is refused; resources outside it are not listed (cluster-scoped ones are listed). Empty means any. |
| `GANTRY_RUNNER_IMAGE` | `forge/gantry/runner:latest` | The runner image (`riveter`, `kubectl`, `gantry-runner`; built from `images/gantry-runner`, tagged with Gantry's version). The estate overlay sets it explicitly. |
| `GANTRY_RUNNER_SERVICE_ACCOUNT` | `gantry-runner` | The Job pod's service account. |
| `GANTRY_RUNNER_CREDENTIALS_SECRET` | `gantry-runner-credentials` | Secret whose keys become the runner's environment (`RIVETER_WAREHOUSE_URL`, `RIVETER_GATEHOUSE_URL`, `RIVETER_CLIENT_ID`, `RIVETER_CLIENT_SECRET`). Required (not optional) by the Job. |
| `GANTRY_AGE_KEY_SECRET` | `gantry-age-key` | Secret with the file `age.key` mounted at `/keys` (optional mount). Not made by the overlay. |
| `GANTRY_JOB_DEADLINE_SECS` | `3600` | `activeDeadlineSeconds` of a Job. A value that is not a number is `3600`. |
| `GANTRY_SELF_DEPLOYMENT` | `forge/gantry` | `namespace/name` of Gantry's own Deployment (applied last, with a rollback). |
| `GANTRY_SELF_ROLLOUT_TIMEOUT_SECS` | `300` | How long the guarded rollout waits before undoing. |
| `GANTRY_PROTECTED` | `forge/gantry,forge/forge-db,forge/redis,forge/gatehouse` | Comma list of `namespace/name` workloads that can never be stopped. Malformed entries are dropped. |

## Local run

| Variable | Default | Meaning |
|---|---|---|
| `GANTRY_FAKE_CLUSTER` | none | JSON file the fake cluster starts from (`none` mode); see [Operations](operations.md#the-simulated-cluster). |
| `GANTRY_PACKAGES_DIR` | none | Directory of `.rivet` files used **instead of** Warehouse (a `riveter pack --out` directory works as is). Files that are not valid packages are skipped with a warning. |
| `GANTRY_VALUES_DIR` | `values` | Local runs read `<dir>/<secret>/env` as a package's variables. |
| `GANTRY_STATE_DIR` | `<temp dir>/gantry` | Where local runners keep `<name>.pid/.log/.exit`. |
| `GANTRY_RUNNER_BIN` | `gantry-runner` | The runner executable a local runner starts. |

## Package registry

| Variable | Default | Meaning |
|---|---|---|
| `WAREHOUSE_URL` | none | Warehouse including its base path; rivets are read from `{WAREHOUSE_URL}/api/v1/rivets` and `/{name}`. |
| `WAREHOUSE_CLIENT_ID` | `gantry-warehouse` | Client of the client-credentials grant (audience `warehouse`). |
| `WAREHOUSE_CLIENT_SECRET` | none | With `GATEHOUSE_URL`: authenticated reads (token from `{GATEHOUSE_URL}/api/v1/token`). Without them: anonymous requests (warning `warehouse: no client credentials (GATEHOUSE_URL, WAREHOUSE_CLIENT_SECRET); asking without a token, which only a Warehouse with auth off will answer`). |
| `GATEHOUSE_URL`, `GATEHOUSE_TLS_VERIFY` | none, `true` | Also used by the registry client; `GATEHOUSE_TLS_VERIFY=false` (the exact string `false`) turns certificate checks off. |

Source selection (`registry::build`): `GANTRY_PACKAGES_DIR` wins; else `WAREHOUSE_URL`; else an empty in-memory registry with the warning `no package source: set WAREHOUSE_URL (with its client credentials) or GANTRY_PACKAGES_DIR; the target list will be empty`.

## Process and platform

| Variable | Meaning |
|---|---|
| `BASE_PATH` | mount prefix |
| `DATABASE_URL` / `POSTGRES_URL` | Postgres (`DB_SCHEMA`, default `gantry`: read by every store statement) |
| `REDIS_URL` | sessions |
| `GATEHOUSE_URL`, `GATEHOUSE_CLIENT_ID`, `GATEHOUSE_CLIENT_SECRET` | sign-in and JWKS; `GATEHOUSE_URL` is required (the JWKS verifier is built at start) |
| `SERVICE_NAME` | the audience a token must carry; unlike conveyor, gantry does not set a default itself, so the platform default `service` applies unless it is set (foreman sets it to the service name; the local compose file sets `gantry`) |
| `SERVICE_AUTH_ENABLED` | authentication on or off |
| `WEB_DIR` | directory with `app.wasm` (default `web`) |
| `KUBERNETES_SERVICE_HOST` | only tested for presence, to detect a pod |

Variables of the runner program only: `GANTRY_PLAN` (required), `GANTRY_VALUES_DIR` (default `/values`), `GANTRY_PACKAGES_DIR` (default `/work/packages`), `GANTRY_NAMESPACES`, `GANTRY_DRY_RUN`, `GANTRY_SOURCE_PACKAGES_DIR`, `RIVETER_AGE_KEY_FILE` (read by `riveter`), and the `RIVETER_*` registry credentials.

## Files

* `i18n/{en-US,de-DE,es-ES,fr-FR,pl-PL}.ftl` in `services/gantry/service/`: compiled into the wasm client; not read at run time.
* `web/app.wasm` (`WEB_DIR`).
* `local/` (`compose.yaml`, `seed.json`, `packages/`, `overlays/`, `pack.sh`, `README.md`): the local demo, see [Operations](operations.md).
* `config/` is empty.
