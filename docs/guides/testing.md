---
title: Testing
parent: Guides
nav_order: 7
---

Forge is tested at four levels: unit tests per crate, Postgres-backed integration tests per service, the cross-service BDD suite `forge-bdd`, and the security suite `forge-sectest`. Continuous integration (`.conveyor.toml`) runs them and a coverage job. This page is checked against the sources of `tests/`, `scripts/` and `.conveyor.toml`.

## Test harness and layout

Crates use `autotests = false` and declare each test target with `harness = false`; the file ends with `quench_test::main!();` (the `quench-test` harness, not libtest). Tests do not live in `src/`. A crate has an entry file `tests/unit.rs` that includes modules from `tests/unit/`, named after the source file they cover:

```rust
// services/workbench/service/tests/unit.rs
#![allow(clippy::wildcard_imports)]
#[path = "unit/authz_tests.rs"]
mod authz_tests;
quench_test::main!();
```

A test in `tests/` is a separate crate and sees only the public API. Services are therefore `[lib]` + `[[bin]]`: the lib holds the modules, `main.rs` is thin. Test entry points per crate type:

| Crate | Entries |
|---|---|
| `*-core` | `tests/unit.rs` |
| `*-app` | `tests/screens.rs` (test renderer, no host) |
| `*-web` | `tests/unit.rs` (stand-in `Transport`) |
| `*-service` | `tests/unit.rs`, `tests/integration.rs`, others per service (`daybook`: `recurrence.rs`; `warehouse`: `pg_storage.rs`, `ui_api_pg.rs`; `sage`: `pg_rows.rs`; `gatehouse`: `pg_models.rs`) |
| Daybook native crates | `host/tests/controller.rs`, `net/tests/unit.rs`, `desktop/tests/unit.rs`, `surface/tests/unit.rs` |

`cargo test --workspace` (or `anvil test --all`) runs them. The harness leaves a JSON/HTML report in `target/debug/quench-test` (`QUENCH_TEST_REPORT=off` disables it, `QUENCH_TEST_REPORT_DIR` moves it); conveyor keeps `summary.json` and `index.html` as artifacts. `cargo nextest` is not used because each test would run in its own process and none could report on the whole run.

## Integration tests that need Postgres

The service integration tests skip themselves (print `skipping ...: <VAR> is not set` and pass) unless a variable points at a database where the service's module is installed. Variables found in the sources:

| Variable | Service |
|---|---|
| `CONVEYOR_TEST_DATABASE_URL` | conveyor |
| `DAYBOOK_TEST_DATABASE_URL` | daybook |
| `FOUNDRY_TEST_DATABASE_URL` | foundry |
| `GANTRY_TEST_DATABASE_URL` | gantry |
| `GATEHOUSE_TEST_DATABASE_URL` | gatehouse |
| `GAUGE_TEST_DATABASE_URL` | gauge |
| `HOIST_TEST_DATABASE_URL` | hoist |
| `SAGE_TEST_DATABASE_URL` | sage |
| `WAREHOUSE_TEST_DATABASE_URL` | warehouse |
| `WORKBENCH_TEST_DATABASE_URL` | workbench |

Run one against a throwaway database:

```bash
docker run --rm -d --name pg-test -e POSTGRES_PASSWORD=postgres -p 5432:5432 pgvector/pgvector:pg18
cargo run -q -p foundry -- apply --catalog services/foundry/service/migrations \
    --database-url postgres://postgres:postgres@127.0.0.1:5432/postgres --install daybook
DAYBOOK_TEST_DATABASE_URL=postgres://postgres:postgres@127.0.0.1:5432/postgres \
    cargo test -p daybook --test integration
docker stop pg-test
```

Use `127.0.0.1`, not `localhost` (the name can resolve to `::1`). Redis-backed cache tests live in Quench (`CACHE_TEST_REDIS_URL`, `CACHE_TEST_REDIS_CLUSTER_URL`) and are skipped unless set.

## Forge BDD (`tests/forge-bdd`)

`forge-bdd` (version 0.1.0, binary `forge-bdd`) is one Cucumber-style runner over `quench-test-bdd`. It starts the services a run needs, waits for them, runs the scenarios against real HTTP, stops the services and prints one summary.

### Suites

Seven suites, each selected by a tag equal to its name (`Target::tag`): `@sage`, `@switchboard`, `@warehouse`, `@gatehouse`, `@conveyor`, `@workbench`, `@daybook`. There are 304 `Scenario` lines in `features/`. Gantry, Gauge, Foundry and Hoist have no BDD suite.

| Suite | Port | Scheme | Features (`features/<service>/`) |
|---|---|---|---|
| sage | 7777 | HTTPS | authentication, chat, chat-extended, conversations, errors, files, models, shutdown, ui_auth, ui_jwt |
| switchboard | 8554 | HTTPS | api_auth, gpu, models, permissions, ui_auth, ui_jwt, vllm |
| warehouse | 8443 | HTTPS | admin, artifacts, crates, docker, docker_registry, files, index, ui_auth, ui_jwt |
| gatehouse | 5443 | HTTP | account_lifecycle, admin_pages, auth_api, home, invites_and_notifications, keys, login_page, oauth, oauth_token, self_service, user_admin |
| conveyor | 9999 | HTTP | api, ui, webhooks |
| workbench | 10443 | HTTP | api, ui |
| daybook | 12443 | HTTP | api |

Redirect listeners: sage 7778, switchboard 8081, warehouse 8080, gatehouse 5080, conveyor 9998, workbench 10080. The sage suite also starts in-process mocks (`src/mocks.rs`): a mock switchboard (checks for a `Bearer` header) and a mock vLLM, on ports 19554 and 18000 (`SWITCHBOARD_URL`, `VLLM_BASE_URL` of the sage process). The ports must be free.

### Run

```bash
cargo run -p forge-bdd                          # all seven suites
cargo run -p forge-bdd -- --tags @gatehouse     # one suite
cargo run -p forge-bdd -- --service warehouse   # same, no tag expression (repeatable)
cargo run -p forge-bdd -- --tags '@sage and not @slow'
cargo run -p forge-bdd -- --name 'refresh'      # filter by scenario name
cargo run -p forge-bdd -- --no-spawn            # services already running: start nothing
foreman test                  # stops the dev estate first, then runs every suite
foreman test gatehouse        # one suite; a bare name means --service
foreman test --tags @sage     # an argument starting with '-' goes to the runner
```

Service selection (`selected_targets`): `--service` names win; otherwise every service name that occurs in the `--tags` expression (or `CUCUMBER_FILTER_TAGS`); otherwise all seven. The suite uses the other cucumber options from `quench_test_bdd::Options`. The exit status is `0` only when every scenario and step passed.

### How a run goes

1. Unless `--no-spawn`, **gatehouse is always started first** and waited for at `http://127.0.0.1:5443/gatehouse/ui/login`, even when `@gatehouse` is not selected: `ForgeWorld::new()` mints test JWTs from gatehouse's `POST /api/v1/test/token`, enabled by `GATEHOUSE_TEST_MODE=true`.
2. The other selected services are started. Each binary is built with `cargo build -p <name>` and run from `target/debug` (not `cargo run`, so killing the runner cannot leave a service holding the port).
3. Readiness: each service is polled every 500 ms up to 60 times (`/ui/login` for sage and gatehouse, `/health` for the others). A service that does not answer yields a warning, not a failure.
4. One pass per service (`Runner::<ForgeWorld>`), filtered so a scenario whose service was not started is skipped. Sage's feature named `Graceful shutdown` kills the process under test, so it runs separately after the rest of sage's pass.
5. All services are stopped and the summary is printed:

```text
═══ Forge BDD summary ═══════════════════════════════════════════════
suite                      scenarios                   steps   result
sage                       ...
sage (shutdown)            ...
...
TOTAL                      ...                                 PASSED
═════════════════════════════════════════════════════════════════════
```

### Environment of the services under test

`common_env` in `src/services.rs` gives every service `SERVICE_NAME`, `SERVER_CERT_PATH=cert.pem`, `SERVER_KEY_PATH=key.pem`, empty `DATABASE_URL` and `POSTGRES_URL`, `ALLOW_IN_MEMORY_DB=true`, `DB_RECREATE=false`, `SERVICE_USERNAME=admin`, `SERVICE_PASSWORD=password`, `AUTH_BOOTSTRAP=true`, `GATEHOUSE_URL=http://127.0.0.1:5443/gatehouse`, `GATEHOUSE_TLS_VERIFY=false`. So the suite needs no Postgres: each service keeps an in-memory store and seeds its own admin in its own realm. Conveyor's `SERVER_CERT_PATH`/`SERVER_KEY_PATH` are pointed at `missing.pem` so it runs plain HTTP. Gatehouse is started with `GATEHOUSE_TEST_MODE=true`, `CLIENTS_CONFIG=tests/forge-bdd/clients.toml` (a copy of the gatehouse client registrations with redirect URIs on the fixed ports), a test `GATEHOUSE_KEY_ENCRYPTION_KEY`, and `CLIENT_SECRET_*` values matching the file (`CLIENT_SECRET_SAGE`, `_SWITCHBOARD`, `_WAREHOUSE` and the others set in `services.rs`).

conveyor, workbench and daybook have no database in the suite: scenarios cover route authentication and request validation, plus the `503` answer when a valid request reaches the in-memory adapter. Daybook's CRUD and owner isolation are covered by `services/daybook/service/tests/integration.rs` against Postgres.

### World and steps

```text
features/<service>/*.feature   each tagged @<service>, background "Given <service> API is available"
src/world.rs                   ForgeWorld: one World for all suites
src/steps/common.rs            steps shared by all services (generic "GET request is sent to ...")
src/steps/<service>[/]         service-specific steps (sage/, switchboard/, warehouse/ directories; gatehouse.rs, conveyor.rs, workbench.rs, daybook.rs)
src/services.rs                build, start, stop; environment
src/mocks.rs                   mock switchboard and vLLM
clients.toml                   OAuth client registrations for the harness's gatehouse
```

Cucumber allows one `World` per run, so `ForgeWorld` unions the state of all suites: the base URL of each service, the last response, and service-specific fields. Base URLs default to the ports above and can be overridden with `SAGE_API_URL`, `SWITCHBOARD_API_URL`, `WAREHOUSE_API_URL`, `GATEHOUSE_API_URL`, `CONVEYOR_API_URL`, `WORKBENCH_API_URL`, `DAYBOOK_API_URL`; `SERVICE_USERNAME`/`SERVICE_PASSWORD` default to `admin`/`password`. `forge-bdd` loads a `.env` next to its `Cargo.toml` if present.

To add a suite: tag the feature `@<service>` with the background step, add the service to `Target` (in `world.rs`) and its start-up to `services.rs`, keep single-service steps under `src/steps/<service>`, and put shared steps in `common.rs`. A duplicate step pattern across files is an ambiguous-match error at run time, not a compile error.

## Security suite (`tests/forge-sectest`)

`forge-sectest` is a black-box scanner (library `forge_sectest`, binary `forge-sectest`) that reads every route macro in the services and in the shared Quench sources (about 480 routes) and attacks each service from outside. Commands: `census` (the attack surface) and `scan --target name=url ...`. Checks: `auth`, `tokens`, `traversal`, `headers`, `cors`, `injection`, `protocol`, `throttle` (`--active` only). Read-only by default; mutating verbs, credential bursts and malformed bodies need `--active`. Targets outside loopback, RFC 1918, `.svc` and `.cluster.local` are refused without `--allow-remote`. Default rate is 20 requests per second (`--rps`). An unreachable target is an error. `tests/forge-sectest/public-routes.txt` is the reviewed baseline of anonymous routes: a new anonymous route fails the scan until it is added or protected (`--write-baseline` regenerates it).

Test targets: `scanners` (the scanners against a vulnerable app, a hardened app, a desync-prone raw server and the real `quench-http`), `selftest`, `census`, `gate` and `live` (both need `--features gate`).

`scripts/security.fish` is the gate:

```fish
scripts/security.fish
FORGE_SECTEST_TARGETS="warehouse=http://127.0.0.1:8080 gatehouse=http://127.0.0.1:8081" scripts/security.fish
```

It sets `QUENCH_TEST_REPORT_DIR=$PWD/target/security`, then runs `cargo test -p forge-sectest --test scanners`, `cargo test -p forge-sectest --features gate --test gate -- --test-threads=1`, and, only when `FORGE_SECTEST_TARGETS` is non-empty, `--test live`. The report is `target/security/index.html`; the exit status is non-zero if any stage failed. The gate cases are `supply_chain_cargo_deny`, `supply_chain_cargo_audit`, `static_unsafe_is_confined_to_allowlisted_files` (allow-list `scripts/unsafe-allowlist.txt`), `static_no_secrets_in_sources`, `static_no_symlink_points_outside_the_checkout` and the scanner self-test. They need `cargo-deny`, `cargo-audit` and the network.

The live test reads `FORGE_SECTEST_TARGETS` (required; space-separated `name=url`), `FORGE_SECTEST_TOKEN`, `FORGE_SECTEST_ACTIVE`, `FORGE_SECTEST_FAIL_ON` (`info|low|medium|high|critical`, default `high`), `FORGE_SECTEST_BASELINE` (default `tests/forge-sectest/public-routes.txt`), `FORGE_SECTEST_RPS`, `FORGE_SECTEST_INSECURE`, `FORGE_SECTEST_ALLOW_REMOTE` and `FORGE_SECTEST_QUENCH_DIR`.

## Continuous integration

`.conveyor.toml` triggers on pushes to `master`, pull requests and a nightly schedule (`0 0 * * *`, Europe/Warsaw). Stages:

| Stage | Jobs | When |
|---|---|---|
| `check` | `format` (`anvil format --check`), `lint` (`anvil lint --all-targets --deny-warnings --json`), `deps` (`anvil machete --json`, `audit --json`, `deny`) | not on schedule |
| `build` | `workspace` (`anvil build --all`) | not on schedule |
| `test` | `unit` (`anvil test --all`), `coverage` | not on schedule |
| `security` | `nightly` (the three `forge-sectest` runs with live targets on cluster DNS, `FORGE_SECTEST_FAIL_ON=medium`, timeout 3600) | schedule only |

### Coverage job

The `coverage` job gives the integration tests a database so their code is counted. It has a sidecar `postgres` (`pgvector/pgvector:pg18`, `POSTGRES_PASSWORD=postgres`, ready when `pg_isready -h 127.0.0.1 -U postgres`) declared with `[[stage.job.service]]`. Its `env` sets `<SERVICE>_TEST_DATABASE_URL` for conveyor, daybook, foundry, gantry, gatehouse, sage, warehouse and workbench to `postgres://postgres:postgres@127.0.0.1:5432/postgres`. Steps:

```toml
steps = [
  { run = "cargo run -q -p foundry -- apply --catalog services/foundry/service/migrations --database-url postgres://postgres:postgres@127.0.0.1:5432/postgres --install conveyor --install daybook --install gantry --install gatehouse --install sage --install switchboard --install warehouse --install workbench" },
  { run = "cargo llvm-cov --workspace --no-report" },
  { run = "cargo llvm-cov report --lcov --output-path lcov.info" },
  { run = "cargo llvm-cov report" },
]
artifacts = ["lcov.info"]
```

The run is report-less; the two reports are made from its profile data. The plain `report` is the table conveyor parses for the repository's scan page.
