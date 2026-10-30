---
title: Configuration
parent: Conveyor
grand_parent: Services
nav_order: 2
---

Every environment variable `conveyor-service` reads, with where it is read. Booleans written `bool` are read with `quench_core::env::is_or`: any value other than empty, `false`, `no` or `0` (case-insensitive) is true, so `banana` is true. Variables of the platform crates (`SERVER_ADDR`, `SERVICE_AUTH_ENABLED`, `GATEHOUSE_*`, `AUTH_*`, `REDIS_URL`, `LOG_*`, ...) are listed once in [Common service runtime](../common-runtime.md); the ones conveyor needs are named below. Secrets are named, never valued.

## Service settings (`src/config/settings.rs`)

Read once at start (`ConveyorConfig::load`). A positive-integer variable that is empty is the default; one that is zero, negative or not a number logs `<KEY>=<value> is not a positive integer, using <default>` and takes the default.

| Variable | Type | Default | Meaning |
|---|---|---|---|
| `CONVEYOR_EXECUTOR` | `native`, `kubernetes` (or `k8s`), `mock`; case-insensitive | `native` | Where steps run. An unknown value logs a warning and means `native`. See [Executors](executors.md). |
| `CONVEYOR_WORK_DIR` | path | `/tmp/conveyor` | Root of the per-run checkouts (`run-<id>/`), removed when the run ends. |
| `CONVEYOR_MAX_CONCURRENT_RUNS` | positive integer | `2` | Worker tasks in this replica, so runs executing at once here. |
| `CONVEYOR_JOB_TIMEOUT_SECS` | positive integer | `3600` | Job timeout when the pipeline sets none. |
| `CONVEYOR_CHECKOUT_TIMEOUT_SECS` | positive integer | `600` | Timeout of each git command of a checkout. |
| `CONVEYOR_CLAIM_STALE_AFTER_SECS` | positive integer | `300` | A running run whose `claimed_at` is older is put back on the queue. Heartbeat is a third of it, the janitor runs at half (both at least 5 s). |
| `CONVEYOR_ALLOW_FORK_PR` | bool | `false` | Build pull requests from forks. A warning is logged at start when this is on with the native executor. |
| `CONVEYOR_BUILD_TAGS` | bool | `false` | Queue runs for tag pushes. |
| `CONVEYOR_HOME_RECENT_RUNS` | positive integer | `5` | Runs on the front page. |
| `CONVEYOR_HOME_MAX_RUNS_PER_REPO` | positive integer | `1` | Cap per repository on the front page. |
| `CONVEYOR_RUNS_PAGE_SIZE` | positive integer | `25` | Runs per page of the history. |

## Secrets and keys

| Variable | Read in | Meaning |
|---|---|---|
| `CONVEYOR_SECRET_KEY` | `secrets::crypto` (per use) | 32 bytes as 64 hex characters or base64 (standard or URL-safe, no padding). Seals pipeline secrets and webhook secrets with XChaCha20-Poly1305; the scope and name are bound into the seal. Unset: secrets cannot be written (`503`), a job that names secrets fails, repository webhook secrets cannot be read (the estate secret is still tried). Generate with `openssl rand -hex 32`. Changing it makes stored values unreadable (`a value could not be decrypted ...`). |
| `CONVEYOR_CREDENTIAL_KEY` | `credentials::store` | Same format, for git credentials; separate so it can rotate on its own. Unset: credentials cannot be written, and checkouts clone unauthenticated. |
| `CONVEYOR_WEBHOOK_SECRET` | `providers` | Estate-wide fallback signing secret for webhooks. With neither this nor a repository `WEBHOOK_SECRET`, deliveries are refused with `503`. |

## Providers and links

| Variable | Default | Meaning |
|---|---|---|
| `CONVEYOR_GITHUB_TOKEN` | unset | Token with `repo:status`. Without it conveyor builds GitHub repositories but reports nothing back (logged once at start). |
| `CONVEYOR_GITHUB_API` | `https://api.github.com` | API base (trailing `/` removed); for GitHub Enterprise. |
| `CONVEYOR_STATUS_CONTEXT` | `conveyor` | The `context` of the commit status. |
| `CONVEYOR_PUBLIC_URL` | empty | Where conveyor is reachable, including its base path (`https://host/conveyor`). Used for the status's `target_url` (`{url}/ui/runs/{id}`) and for the link in run-result mail; without it mail is not queued. |

## Warehouse (artifacts)

Artifacts are kept only when `WAREHOUSE_URL` is set (`WarehouseStore::from_env`).

| Variable | Default | Meaning |
|---|---|---|
| `WAREHOUSE_URL` | empty | Warehouse including its base path. Empty: nothing is kept, the tests/security checks never appear. |
| `WAREHOUSE_PUBLIC_URL` | empty | What a browser reaches warehouse at, when that differs; replaces the `WAREHOUSE_URL` prefix in artifact links (`href`). |
| `WAREHOUSE_CLIENT_ID` | `conveyor-publish` | Client of the client-credentials grant. |
| `WAREHOUSE_CLIENT_SECRET` | empty | With `GATEHOUSE_URL` set too: identity is a bearer token from `{GATEHOUSE_URL}/api/v1/token` (cached until 30 s before expiry). Preferred. |
| `WAREHOUSE_TECH_USERNAME`, `WAREHOUSE_TECH_PASSWORD` | empty | Fallback: HTTP basic authentication (when no client secret). With neither, requests are anonymous. |
| `WAREHOUSE_TLS_VERIFY` | `true` | bool; false accepts invalid certificates. |
| `CONVEYOR_ARTIFACT_STORAGE` | `artifacts` | The warehouse file storage name: files go to `{WAREHOUSE_URL}/api/v1/files/{storage}/file?path=...`. |

## Gatehouse

| Variable | Meaning |
|---|---|
| `GATEHOUSE_URL`, `GATEHOUSE_CLIENT_ID`, `GATEHOUSE_CLIENT_SECRET`, `GATEHOUSE_TLS_VERIFY` | The service's own OAuth client; sign-in and JWKS (required: the service panics at start without `GATEHOUSE_URL`: `failed to set up the JWKS verifier (is GATEHOUSE_URL set?)`). |
| `CLIENT_SECRET_CONVEYOR_GATEHOUSE` | The secret of client `conveyor-gatehouse`. With `GATEHOUSE_URL` it switches run-result mail on (otherwise `CLIENT_SECRET_CONVEYOR_GATEHOUSE is not set: run notifications are switched off` is logged). |

## Kubernetes executor (`executors/kubernetes.rs`, `executors/manifest.rs`)

Used only with `CONVEYOR_EXECUTOR=kubernetes`. Integers that do not parse take the default.

| Variable | Default | Meaning |
|---|---|---|
| `CONVEYOR_K8S_NAMESPACE` | the pod's own namespace (`/var/run/secrets/kubernetes.io/serviceaccount/namespace`), else `default` | Where jobs are created. |
| `CONVEYOR_K8S_DEFAULT_IMAGE` | `alpine:3.22` | Image of a job whose pipeline names none. |
| `CONVEYOR_K8S_GIT_IMAGE` | `alpine/git:latest` | Image of the checkout init container. |
| `CONVEYOR_K8S_SERVICE_ACCOUNT` | none | `serviceAccountName` of the job pods. |
| `CONVEYOR_K8S_TTL_SECONDS` | `3600` | `ttlSecondsAfterFinished`. |
| `CONVEYOR_K8S_POD_START_SECS` | `120` | How long a pod may stay `Pending`. |
| `CONVEYOR_K8S_SERVICE_READY_SECS` | `300` | How long a job's services get to become ready; added to the pod start time for such a job. |

The Kubernetes client is configured by `Client::infer` (in-cluster environment, else `KUBECONFIG`/`~/.kube/config`).

## Process and platform

| Variable | Default | Meaning |
|---|---|---|
| `BASE_PATH` | `/` | Mount prefix; the API prefix wrapped by `Auth` is `{BASE_PATH}/api/v1`. |
| `SERVICE_NAME` | set to `conveyor` by `main` when unset | The audience a token must carry (`Claims::allows(service_name)`). |
| `DB_SCHEMA` | set to `conveyor` by `main` when unset | Schema the service reads and writes; every statement is formatted with it. |
| `DATABASE_URL` / `POSTGRES_URL` | none | Postgres. Without one the process panics unless `ALLOW_IN_MEMORY_DB` is true; in-memory means no worker pool, no clock, every queue call `503`. |
| `AUTH_DB_SCHEMA` | `auth` | Realm schema (the `users` table `UserDb` reads). |
| `HOSTNAME` | `conveyor` | Part of the worker ids. |
| `WEB_DIR` | `web` | Directory with `app.wasm` (the image build puts it there, `WASM_PACKAGE=conveyor-web`). |
| `SERVICE_AUTH_ENABLED` | `false` | Authentication on or off (platform default is off). |
| `REDIS_URL` | none | Session store (in-process when unset). |

## Files

* `i18n/{en-US,de-DE,es-ES,fr-FR,pl-PL}.ftl` under `services/conveyor/service/`: compiled into the wasm client (`include_str!`); the service does not read them at run time.
* `web/app.wasm` (`WEB_DIR`): the client module; a page request answers `404` `the browser client is not installed on this server` without it.
* `logs/` (`LOG_DIR`): daily log files `app.<date>.log`.
* `config/` is empty (a `.gitkeep`); conveyor reads no configuration file.
* `.env` in the service directory: `foreman.toml` takes `GATEHOUSE_CLIENT_SECRET` from it (`client_secret_conveyor`). Its contents are not documented here.
