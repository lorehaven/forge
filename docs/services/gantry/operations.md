---
title: Operations
parent: Gantry
grand_parent: Services
nav_order: 4
---

Run, deploy, observe and test Gantry. Nothing here contacts a real cluster; commands that would are described, not run.

## Running it locally

Everything that reaches outside the service is behind a trait chosen by environment, so Gantry runs on a laptop with no cluster, and with one against a scratch namespace.

| Setup | `GANTRY_CLUSTER` | `GANTRY_RUNNER` | What happens |
|---|---|---|---|
| Nothing set | `none` | `dry-run` | An empty fake cluster; operations run `gantry-runner` as a child process that prints every mutating command and changes nothing. |
| Click-through demo | `none` | `simulate` | The fake cluster is seeded (`GANTRY_FAKE_CLUSTER`) and each step is applied to it in memory, so start, stop, swap and upgrade can be watched taking effect. Runs no `riveter` and no `kubectl`. |
| Real scratch namespace | `kubeconfig` | `local` | Reads the cluster with your kubeconfig (`GANTRY_KUBE_CONTEXT` pins a context); operations run `gantry-runner` as a child process with **your own** `kubectl` and `riveter`. Pair it with `GANTRY_NAMESPACES=<scratch>` and `GANTRY_PACKAGES_DIR` so nothing outside is touched. |
| Real, dry | `kubeconfig` | `dry-run` | Reads a real cluster, prints what it would change. |
| In a pod | `in-cluster` | `job` | The production mode: the pod's service account reads, a Job runs each operation. |

```text
SERVICE_AUTH_ENABLED=false BASE_PATH=/gantry DATABASE_URL=... GATEHOUSE_URL=... \
GANTRY_CLUSTER=kubeconfig GANTRY_RUNNER=local GANTRY_NAMESPACES=scratch \
GANTRY_PACKAGES_DIR=./packages gantry-service
```

Postgres (the `gantry` foundry module) and Redis are required in every mode. Under foreman the service is `foreman start gantry` (port 11443, redirect 11080, base `/gantry`, needs gatehouse); the cluster settings are not set there, so it runs `none` + `dry-run`. See [Configuration](configuration.md) for the combinations that are refused.

### The demo (`services/gantry/service/local/`)

```bash
cargo build -p gantry-service -p foundry-service      # workspace root
services/gantry/service/local/pack.sh                   # builds the demo .rivet packages; needs riveter on PATH or RIVETER=...
cd services/gantry/service/local && docker compose up
```

`compose.yaml` runs `pgvector/pgvector:pg18`, `redis:7-alpine`, a one-shot `migrate` (`foundry-service --install gantry apply`) and the service (your built binary mounted into an Arch image, port `127.0.0.1:11443`) with `SERVICE_AUTH_ENABLED=false`, `BASE_PATH=/gantry`, `SERVER_CERT_PATH=/nonexistent` (plain HTTP), `GANTRY_CLUSTER=none`, `GANTRY_RUNNER=simulate`, `GANTRY_FAKE_CLUSTER=/app/local/seed.json`, `GANTRY_PACKAGES_DIR=/app/local/packages`. Open `http://localhost:11443/gantry/ui/home`. The demo packages are `ml` and `media` (each at 1.0.0 and 1.1.0, from `local/overlays/`); `*.rivet` files are not committed. What to try (`local/README.md`): start training (inference is stopped first, one operation); stop is delete (Sage shows `missing`, Apply brings it back); edit a ConfigMap (the row turns `edited`); upgrade `ml` while training is stopped (it stays stopped). A restart of the container resets the fake cluster to the seed while operations, recorded states and the inventory stay in Postgres, so rows can show drift until `docker compose down -v`.

### The simulated cluster

`GANTRY_FAKE_CLUSTER` names a JSON file, either an array (taken as the workloads) or an object:

| Key | Items | Meaning |
|---|---|---|
| `workloads` | `{kind, name, namespace, package, version?, desired, ready}` | live Deployments/StatefulSets/DaemonSets of packages |
| `resources` | `{apiVersion, kind, name, namespace?, package, version?, ready?, edited?, owned?}` | live resources of other kinds |
| `declared` | same shape as `resources` | declared by a package but not live (stopped or never applied) |

Everything in the file becomes part of its package's **catalog**: what the simulating runner puts back when something is applied. At start, for every seeded package with a version, the catalog is recorded as the package's inventory, as if it had been installed. The simulator's step semantics: `pull` and `check` print simulated lines (`check` prints a `riveter-inventory:` line of the package's items: the catalog, the deployments of the published manifest, the live workloads, or a single Deployment named after the package); `install` puts the selected items (all, or `targets`, minus `except`) into the fake cluster at the version, workloads at their `replicas` or 1; `scale`, `delete`, `apply_yaml` change the fake; `restart_users`, `delete_pods`, `wait_gone`, `rollout` only print. The log is written in the runner's style and ends with the `GANTRY-RESULT:` line.

## In the cluster

* Image: `anvil docker` module `gantry-service` (`.anvil/docker.toml`): image name `gantry`, registries `ossiriand.arda:8080/forge` and `ennor.ddns.net/forge`, build arguments `RESOURCE_DIR=web`, `WASM_PACKAGE=gantry-web`.
* The runner image is separate: `images/gantry-runner/Dockerfile` builds `riveter` and `gantry-runner` (musl) from this workspace plus `kubectl` into an alpine image. `foreman run build-gantry-runner` / `publish-gantry-runner` (`scripts/image.sh gantry-runner build|publish`); tag `ossiriand.arda:8080/forge/gantry/runner:<gantry-service version>` (the service names the image, the two are released together). The registry token for the crate registry is passed as a build argument when `ENNOR_TOKEN` is set; pushing needs `docker login ossiriand.arda:8080`. *Note:* the guides report that the estate overlay's `GANTRY_RUNNER_IMAGE` can lag the service version.
* The estate overlay (`homecloud`, package `forge`, directory `gantry/`): ConfigMap, Secret, Deployment, Service, Ingress, a Role, a ServiceAccount and `runner-credentials.yaml.j2`. The Secret `gantry-age-key` (file `age.key`) is the one thing not in git: `kubectl -n forge create secret generic gantry-age-key --from-file=age.key=$HOME/.config/riveter/age.key`.
* Probes: readiness `GET /gantry/health/ready`, liveness `GET /gantry/health/live`.
* One replica: the reconciler runs in every replica and the unique index keeps operations serial, but nothing coordinates two replicas beyond that, and the local-runner modes are single-process.

## Security model

Managing every overlay means the runner needs, in effect, cluster-wide write access. That is accepted and contained: the **service** account can create and read Jobs in its namespace and `get`/`list` a fixed set of kinds cluster-wide (never Secrets, nothing that changes anything); the broad runner account is bound only to the Job, which exists only while an operation runs. Every operation records who asked, the plan and the outcome. A package's variables, and its secrets (age-encrypted inside the package), are opened only inside the runner Job, with a key mounted from a Secret; they never pass through the service, its database or its API. Plans are data: a closed set of steps, each value validated and passed as its own argument. The namespace allow-list and the protected-workload list apply to every submission, whatever route it came by.

## Health

From `quench-starter`: `GET /health` (`200 OK`), `GET /health/live` (`200 {"status":"live"}`), `GET /health/ready` (`503 {"status":"not_ready"}` until gatehouse's `/health/ready` answers, then `200 {"status":"ready"}`). They are not in `openapi.yaml`. No metrics endpoint. Logging: `quench-log` (`RUST_LOG`, `LOG_DIR`, `app.<date>.log`); startup lines `cluster: <mode>, runner: <mode>, namespaces: [...]`; the reconciler logs `reconciler: <report>` at debug, `operation <id>: runner status unavailable: <error>` and `operation <id>: runner missing, starting it again` at warn.

## Failure modes

| Symptom | Cause and behaviour |
|---|---|
| The process panics at start with `configuration: ...` | a refused `GANTRY_CLUSTER`/`GANTRY_RUNNER` combination or an unknown value |
| Panics with `cluster: ...` | the kubeconfig or in-cluster configuration could not be loaded |
| Panics `failed to set up the JWKS verifier (is GATEHOUSE_URL set?)` | `GATEHOUSE_URL` missing |
| The target list is empty | no package source (`WAREHOUSE_URL` / `GANTRY_PACKAGES_DIR`); logged at start |
| Target and resource calls `502` | the registry or the cluster is unreachable or refused |
| An operation stays `running` | the cluster cannot be reached (the reconciler leaves it alone), or the runner is genuinely running (up to `GANTRY_JOB_DEADLINE_SECS`) |
| Operation `failed`: `could not start the runner: ...` | the Job could not be created (RBAC, quota, a bad image name is only seen later as a failed Job) |
| `failed` with a step message | the first failing command; the log has `!! step <i> (...)` and the command |
| `rolled_back` | only the guarded rollout of Gantry's own Deployment: the new pod did not become ready |
| A plan cannot be confirmed: `409` | already confirmed, or the cluster moved since it was planned |
| A plan stuck with `operation_id = 'claiming'` | the service died between claiming a plan and storing the operation id; the plan cannot be confirmed again (see the report) |
| A stopped deployment comes back after an upgrade | it was not recorded as stopped (the stop failed, or the state was never recorded) |
| Resource list is slow | each `GET /api/v1/resources` lists every discovered kind (8 at a time) |

## Limits

* One running operation at a time (scope `cluster`); the rest queue. The reconciler claims at most 16 per tick.
* A Job runs at most `GANTRY_JOB_DEADLINE_SECS` (3600).
* Edited YAML at most 128 KiB (it travels in the Job's environment).
* Stored log: 64 KiB tail. Operation list: 200 at most per call.
* Discovery is cached for 60 seconds; the reconciler ticks every 3 seconds.

## Tests

`services/gantry/service/tests/`: `unit` (planner, deployments, resources and the direct actions, the reconciler including adoption after a simulated restart, the runner loop with a recording executor, the local executor with stand-in scripts, the Kubernetes adapter against a scripted API, the API and authorisation, settings, simulation), `pg_store` (the real-Postgres store: skipped unless `GANTRY_TEST_DATABASE_URL` points at a throwaway database) and `cluster_it` (talks to a real cluster; ignored unless run with `--ignored` and `GANTRY_IT_NAMESPACE`, `GANTRY_IT_IMAGE` against a scratch namespace; its module docs say how). All use the custom harness. Unit tests use in-memory parts: a fake cluster that scripts Jobs and workloads, a memory store, a memory registry. The real-Postgres behaviour (the one-running-per-scope index, a kill -9 mid-operation) and real-cluster behaviour (install with the real `--inventory` line, discovery of every kind, delete and apply back, a ConfigMap edit that restarted exactly one Deployment, stop/start as delete/apply, update while stopped, stale-plan refusal, self-upgrade with rollback) were, per the previous documentation, verified by hand in scratch namespaces, not in CI. *Unverified here.* There is no Gantry suite in `forge-bdd`. See [Testing](../../guides/testing.md).
