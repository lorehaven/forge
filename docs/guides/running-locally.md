---
title: Running the estate locally
parent: Guides
nav_order: 5
---

`foreman` reads `foreman.toml` at the workspace root, starts Postgres and Redis in containers, applies the schemas with Foundry, builds the browser clients and runs the services from `target/debug`. Everything below is copied from that file; the command reference is the [Foreman](../cli/foreman/index.md) page.

## Commands

```text
foreman                  start postgres and redis, install the schemas, run every service
foreman start conveyor   start only conveyor and what it needs
foreman repl             pick services interactively, then start them
foreman stop             stop the services (the containers keep running)
foreman stop conveyor    stop one of them
foreman stop all         stop the services and the containers under them
foreman status           what is up, and on which port
foreman logs sage        follow one service's log
foreman db               postgres + the schema catalog, no services
foreman reset            drop every schema foundry owns and reinstall (dev only)
foreman test [service]   run every BDD suite, or one (cucumber flags: --tags, --name)
foreman run <task>       run a task by hand (web, build-rust-builder, publish-rust-builder, build-gantry-runner, publish-gantry-runner)
```

Services are built with `cargo build -q --all-features --package ${package}` and launched from `target/debug/${package}`, not through `anvil run`, so that foreman holds a real PID per service. Naming services starts a subset: only those packages are built and Foundry installs only their schemas (`--install <service>` per name). What a service cannot start without comes with it (`needs`).

## What it sits on

| Container | Image | Port | Ready check |
|---|---|---|---|
| `postgres` | `pgvector/pgvector:pg18` | 5432 | `pg_isready -U postgres` (30 s) |
| `forge-redis` | `redis:7-alpine` | 6379 | `redis-cli ping` (15 s) |

Postgres is `postgres`/`postgres`, database `postgres`. Redis holds the sessions: expiry is the TTL, revocation is a delete, so a logout is immediate in every service.

## Tasks before a start

| Task | Role | What it does |
|---|---|---|
| `foundry` | migrate | `foundry-service --catalog services/foundry/service/migrations --config services/foundry/service/config/install.toml [--install <svc> ...] apply` with `DATABASE_URL`. |
| `web` | migrate | `bash scripts/web.sh`: builds every `services/<n>/web` crate for `wasm32-unknown-unknown` (`--profile wasm`) and copies it to `services/<n>/service/web/app.wasm`. |
| `seed` | migrate | Applies every `testdata/*.sql` through `docker exec -i postgres psql` (extra users with a spread of permissions, workbench, conveyor and sage demo data). Idempotent (`ON CONFLICT DO NOTHING`); files for a schema that is not installed skip themselves. |
| `reset` | reset | `foundry-service ... reset --yes` after stopping the services. |
| `build-rust-builder`, `publish-rust-builder`, `build-gantry-runner`, `publish-gantry-runner` | by hand | `scripts/image.sh <image> build|publish`. See [Deployment](deployment.md). |

## Services

All services run with `workdir = services/<name>/service` (their `.env`, `i18n`, `web` and `config` are resolved relative to it), scheme `https`, host `localhost`, health path `/health`, start timeout 30 s, stop timeout 4 s, and the shared certificate pair from `certs/` (`SERVER_CERT_PATH`, `SERVER_KEY_PATH`).

| Service | Package | HTTPS port | HTTP redirect | `BASE_PATH` | `needs` |
|---|---|---|---|---|---|
| gatehouse | `gatehouse-service` | 5443 | 5080 | `/gatehouse` | - |
| warehouse | `warehouse-service` | 6443 | (default) | `/warehouse` | gatehouse |
| switchboard | `switchboard-service` | 7443 | (default) | `/switchboard` | gatehouse |
| sage | `sage-service` | 8443 | (default) | `/sage` | gatehouse, switchboard |
| conveyor | `conveyor-service` | 9443 | (default) | `/conveyor` | gatehouse |
| workbench | `workbench-service` | 10443 | (default) | `/workbench` | gatehouse |
| gantry | `gantry-service` | 11443 | 11080 | `/gantry` | gatehouse |
| daybook | `daybook-service` | 12443 | 12080 | `/daybook` | gatehouse |
| gauge | `gauge-service` | 13443 | 13080 | `/gauge` | gatehouse |
| hoist | `hoist-service` | 14443 | 14080 | `/hoist` | gatehouse |
| lectern | `lectern-service` | 15443 | 15080 | `/lectern` | gatehouse, warehouse |

Services without `SERVER_HTTP_REDIRECT_ADDR` in their block use the starter default `0.0.0.0:80`; the ones that set it avoid sharing one redirect port. Gatehouse, switchboard, conveyor, workbench, daybook, gantry, gauge, hoist and lectern borrow the certificate of `services/warehouse/service` (`cert_from`); Gatehouse ships none of its own, and Switchboard is pointed at the shared one with `SERVER_CERT_PATH`/`SERVER_KEY_PATH`.

Sign in at `https://localhost:5443/gatehouse/ui/login` with the admin account (see below).

### Environment common to all services (`[defaults.env]`)

| Variable | Value |
|---|---|
| `DATABASE_URL` | `${database_url}` = `postgres://postgres:postgres@localhost:5432/postgres` |
| `SERVICE_AUTH_ENABLED` | `true` |
| `AUTH_DB_SCHEMA` | `auth` |
| `SERVER_ADDR` | `0.0.0.0:${port}` |
| `BASE_PATH` | `${base_path}` |
| `SERVICE_NAME` | `${name}` |
| `REDIS_URL` | `redis://localhost:6379` |
| `RUST_LOG` | `info` |
| `GATEHOUSE_URL` | `https://localhost:5443/gatehouse` |
| `GATEHOUSE_CLIENT_ID` | `${name}` |
| `GATEHOUSE_TLS_VERIFY` | `false` |

Gatehouse unsets `GATEHOUSE_URL` and `GATEHOUSE_CLIENT_ID` (it is the destination).

### Per-service additions

* **gatehouse**: `AUTH_BOOTSTRAP=true`, `SERVICE_USERNAME`/`SERVICE_PASSWORD` (the admin), `GATEHOUSE_KEY_ENCRYPTION_KEY`, one `CLIENT_SECRET_<NAME>` per client in `clients.toml` (sage, switchboard, warehouse, conveyor, workbench, gantry, gauge, gantry-warehouse as `CLIENT_SECRET_GANTRY_WAREHOUSE`, `CLIENT_SECRET_SAGE_SWITCHBOARD`), `AUTH_REDIRECT_HOSTS` (the `https://localhost:<port>` of 6443, 7443, 8443, 9443, 10443, 11443, 13443), `SERVER_HTTP_REDIRECT_ADDR=0.0.0.0:5080`, and the `*_UI_URL` of every service (`https://localhost:<port>/<name>/ui/home`; Daybook's is `.../daybook/ui`).
* **warehouse**: `GATEHOUSE_CLIENT_SECRET`, `DOCKER_TOKEN_SECRET`.
* **switchboard**: `GATEHOUSE_CLIENT_SECRET`, `SERVER_CERT_PATH`, `SERVER_KEY_PATH`. A `pre_stop` hook (60 s timeout, 3 s settle) asks Switchboard to delete its vLLM instances (using the `sage-switchboard` client) because they are not foreman's children.
* **sage**: `GATEHOUSE_CLIENT_SECRET`, `CLIENT_SECRET_SAGE_SWITCHBOARD`. With `FORGE_SKIP_MODELS` set in foreman's environment, `SAGE_DEFAULT_MODELS=[]` (the GPU stays idle and the "initializing" screen does not appear).
* **conveyor, workbench, gantry, gauge**: `GATEHOUSE_CLIENT_SECRET`. gantry and gauge also set a redirect address.
* **daybook**: only `SERVER_HTTP_REDIRECT_ADDR=0.0.0.0:12080` (no client secret: it has no OAuth client).
* **lectern**: `WAREHOUSE_URL=https://localhost:6443/warehouse`, `WAREHOUSE_CLIENT_ID=lectern-warehouse`, `WAREHOUSE_CLIENT_SECRET`, `WAREHOUSE_TLS_VERIFY=false`. It serves what `anvil docs release` put in Warehouse's `docs` storage.
* **hoist**: `DB_SCHEMA=hoist`, `HOIST_APTOIDE_PAGES=5` (keeps the first catalogue refresh short; the refresh fetches three upstream stores over the network).

### Values

`[vars]` in `foreman.toml`. Secrets are read from each service's `.env` (`env_file` + `key`) and fall back to the `default` shown, so a fresh checkout works without any `.env`.

| Var | Source key | Default |
|---|---|---|
| `key_encryption_key` | `GATEHOUSE_KEY_ENCRYPTION_KEY` in gatehouse `.env` | `forge-local-dev-key-encryption-key` |
| `docker_token_secret` | `DOCKER_TOKEN_SECRET` in warehouse `.env` | `forge-local-dev-docker-token-secret` |
| `admin_user` | `SERVICE_USERNAME` in switchboard `.env` | `admin` |
| `admin_password` | `SERVICE_PASSWORD` in switchboard `.env` | `password` |
| `client_secret_<svc>` | `GATEHOUSE_CLIENT_SECRET` in that service's `.env` | `forge-local-dev-secret-<svc>` |

After a start foreman prints the login URL and `admin / password` (or the configured values). It also warns about running `vllm serve` processes that outlive a stop (`pgrep`), and does not kill them.

## Tests share the ports

`foreman test` stops the estate first: the BDD suite starts its own copies of the services on the same ports. See [Testing](testing.md).

## Gantry without a cluster

Gantry reads and changes a Kubernetes cluster, so it has two local modes that never touch one by default. Environment (from `services/gantry/service/src/`; the Gantry page has the full list):

| Variable | Values | Effect |
|---|---|---|
| `GANTRY_CLUSTER` | `in-cluster` (default in a pod), `kubeconfig`, `none` | How the cluster is read. `none` is an in-memory fake. `GANTRY_KUBE_CONTEXT` pins a context. |
| `GANTRY_RUNNER` | `job` (default in a pod), `local`, `dry-run`, `simulate` | How operations run. `local` runs a child process with your `kubectl` and `riveter`; `dry-run` prints mutating commands and changes nothing; `simulate` (fake cluster only) applies each step to the in-memory cluster. |
| `GANTRY_FAKE_CLUSTER` | JSON file | Initial fake cluster: `workloads`, `resources`, `declared`. |
| `GANTRY_NAMESPACES` | list | Allow-list; empty means any. |
| `GANTRY_PACKAGES_DIR` | directory | `.rivet` files instead of Warehouse. |
| `GANTRY_VALUES_DIR` | directory | Local runs read `<dir>/<secret>/env`. |
| `GANTRY_STATE_DIR` | directory | Where local runners keep their log and exit code. |

With nothing set a run is `none` + `dry-run`. A write to a real cluster must be said twice: `GANTRY_CLUSTER=kubeconfig` and `GANTRY_RUNNER=local`; `local` with anything but `kubeconfig` is refused, and `job` with no cluster is refused. Postgres (the `gantry` Foundry module) and Redis are still required.

The click-through environment is `services/gantry/service/local/`: `compose.yaml` (Postgres `pgvector/pgvector:pg18`, Redis, a `migrate` container running `foundry-service --install gantry apply`, and the service from your built `target/debug` binaries on `127.0.0.1:11443`), `pack.sh` (builds the demo `.rivet` packages `ml` and `media`, 1.0.0 and 1.1.0, with `riveter`), and `seed.json`. The compose file sets `SERVICE_AUTH_ENABLED=false`, `BASE_PATH=/gantry`, plain HTTP (`SERVER_CERT_PATH=/nonexistent`), `GANTRY_CLUSTER=none`, `GANTRY_RUNNER=simulate`, `GANTRY_FAKE_CLUSTER=/app/local/seed.json`, `GANTRY_PACKAGES_DIR=/app/local/packages`.

```bash
cargo build -p gantry -p foundry
services/gantry/service/local/pack.sh
cd services/gantry/service/local && docker compose up
# then http://localhost:11443/gantry/ui/home
```

## The native apps against a local estate

The shared dev certificate lives in `certs/` (`cert.pem` and `key.pem`, not committed; see `certs/README.md`) and foreman gives every service its path in `SERVER_CERT_PATH` and `SERVER_KEY_PATH`. It is issued for `registry.local`, not `localhost` (the reason for `GATEHOUSE_TLS_VERIFY=false` above). How the Daybook, Hoist and Forklift apps are pointed at a local Gatehouse is on their pages.
