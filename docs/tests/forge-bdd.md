# Forge BDD

`forge-bdd` (`tests/forge-bdd`) is the workspace's single Cucumber-based BDD runner, covering every Forge service's behavior — sage, switchboard, warehouse, gatehouse, conveyor and workbench — from one binary and one `World`. It exists so integration-level scenarios don't fragment across six separate test crates: it starts the services a run needs, waits for them to answer, runs the matching `.feature` scenarios against real HTTP, and tears the services back down, closing with one consolidated pass/fail report.

## What it covers

- **Six services, six tags**: `@sage` (7777, HTTPS, plus mock switchboard/vLLM backends), `@switchboard` (8554, HTTPS), `@warehouse` (8443, HTTPS), `@gatehouse` (5443, plain HTTP — ships no test certs), `@conveyor` (9999, plain HTTP — `SERVER_CERT_PATH`/`SERVER_KEY_PATH` are pointed at `missing.pem`), `@workbench` (10443, plain HTTP — same certless setup as conveyor).
- **Auth boundaries, per service**: every service has an `ui_auth` / `ui_jwt` pair (login → gatehouse delegation, logout, `/ui/status` honesty, and every way a session token can fail to authenticate a protected page) or an `api_auth` feature (the bare "no token at all" path the other features' backgrounds skip past). Gatehouse additionally covers `/.well-known/jwks.json`, signing-key rotation, and the `/api/v1/token` grants that need no browser; warehouse covers the Docker Registry v2 bearer handshake and the artifacts/apk `RequireWrite` split.
- **`features/<service>/*.feature`**, each tagged `@<service>` and each with a `Given <service> API is available` background step so the shared, generic steps (`src/steps/common.rs`) know which base URL to hit.
- **One `ForgeWorld`** (`src/world.rs`) unions state that used to live in separate per-service worlds: base/API URLs for all six services, the last HTTP response, and service-specific fields (sage's `jwt_token`/conversation id, switchboard's `switchboard_token`, warehouse's crate/docker fields, gatehouse's session/refresh/admin tokens).
- **Gatehouse-issued auth for everything**: every service now verifies bearer tokens against gatehouse's JWKS rather than a shared secret. Gatehouse is always started and health-checked first — even for a run that didn't select `@gatehouse` — because `ForgeWorld::new()` mints its test JWTs from gatehouse's `POST /api/v1/test/token` (enabled by `GATEHOUSE_TEST_MODE=true`).
- **In-memory-only services**: `services.rs` starts each service with `ALLOW_IN_MEMORY_DB=true` and `AUTH_BOOTSTRAP=true`, so the whole suite needs no Postgres — each service seeds its own admin user in its own throwaway realm, unlike production where gatehouse alone seeds users.
- **`clients.toml`**: OAuth client registrations (`sage`, `switchboard`, `warehouse`, `conveyor`, plus `sage-switchboard` for sage's machine-to-machine `client_credentials` flow) mirroring `docker/gatehouse-service/config/clients.toml`, with redirect URIs pointed at the fixed local ports the harness uses.
- **Mocks for sage** (`src/mocks.rs`): an in-process mock switchboard and mock vLLM server so the `@sage` suite doesn't depend on the real switchboard service; the mock switchboard checks for a `Bearer` auth header, matching sage's real client-credentials flow.
- **Graceful shutdown scenario**: sage's shutdown feature terminates the sage process under test, so it always runs last, alone, after the rest of the `@sage` suite.
- **conveyor and workbench run with no database**: both need Postgres for their real work (conveyor's queue, workbench's tables), which this suite deliberately doesn't provide, so their scenarios cover the UI shell, gatehouse delegation, which routes require a token (GET *and* the write verbs), the honest `503`/`Postgres` the domain layer answers with, and — for conveyor — everything the webhook endpoint decides before it looks a repository up (the provider, the event type, the body's shape, the ref). The queue and table behaviour themselves live in `docker/conveyor-service/tests/integration` and `docker/workbench-service/tests/integration`, against a real Postgres.

## Layout

```
features/<service>/*.feature   tagged @<service>
src/world.rs                   one World for every suite
src/steps/common.rs            steps shared by all services
src/steps/<service>/           service-specific steps
src/services.rs                how each service is built, started and stopped
src/mocks.rs                   mock switchboard and vLLM, for sage
```

Cucumber allows one `World` per run, so the state that used to live in three separate crates' worlds is unioned into `ForgeWorld`. Generic steps ("GET request is sent to …") address whichever service the feature's `Given <service> API is available` background step selected, which is why the same step phrasing works across every service's features.

## How to run it

Directly:

```bash
cargo run -p forge-bdd                          # all services
cargo run -p forge-bdd -- --tags @gatehouse     # one service
cargo run -p forge-bdd -- --service warehouse   # same, without a tag expression
cargo run -p forge-bdd -- --tags '@sage and not @slow'
cargo run -p forge-bdd -- --name 'refresh'      # filter by scenario name
```

Or through `foreman`, which stops the local development estate first (the suite starts its own service copies on the same ports, so the two can't run concurrently):

```bash
foreman test                 # every suite
foreman test gatehouse       # one suite; a bare name means --service
foreman test --tags @sage    # anything starting with `-` goes to cucumber
```

`--service` explicitly says which services to start; without it, the runner infers services from the services named in `--tags`; with neither, it runs all six. The exit code is `0` only when every scenario passed, so CI can run the binary directly.

Each service is run as a separate pass, and the run closes with a consolidated report, e.g.:

```
═══ Forge BDD summary ═══════════════════════════════════════════════
suite                      scenarios                   steps   result
sage                       63 passed              231 passed   ok
sage (shutdown)              1 passed                3 passed   ok
switchboard                40 passed              198 passed   ok
warehouse                  46 passed              208 passed   ok
gatehouse                  82 passed              562 passed   ok
conveyor                   28 passed              100 passed   ok
workbench                  25 passed               91 passed   ok
─────────────────────────────────────────────────────────────────────
TOTAL                     285 passed             1393 passed   PASSED
═════════════════════════════════════════════════════════════════════
```

## Requirements

- No external database — every service runs against an in-memory store for this suite.
- Each service binary (`sage-service`, `switchboard-service`, `warehouse-service`, `gatehouse-service`, `conveyor-service`) is built via `cargo build -p <name>` and launched from `target/debug` (not `cargo run`, so a leaked process doesn't hold the port for the next run).
- Fixed local ports: 7777 (sage), 8554 (switchboard), 8443 (warehouse), 5443 (gatehouse), 9999 (conveyor), 10443 (workbench) — these must be free.

## Configuration

- `.env` sets `SERVICE_USERNAME`/`SERVICE_PASSWORD` and the `*_API_URL` variables the `World` reads as overridable defaults (`SAGE_API_URL`, `SWITCHBOARD_API_URL`, `WAREHOUSE_API_URL`, `GATEHOUSE_API_URL`; `CONVEYOR_API_URL` and `WORKBENCH_API_URL` default to `http://127.0.0.1:9999` / `:10443` in code if unset).
- `clients.toml` is pointed at via `CLIENTS_CONFIG` when the harness starts gatehouse; its `secret_env` fields (`CLIENT_SECRET_SAGE`, `CLIENT_SECRET_SWITCHBOARD`, `CLIENT_SECRET_WAREHOUSE`, `CLIENT_SECRET_CONVEYOR`, `CLIENT_SECRET_SAGE_SWITCHBOARD`) are matched by identically-named env vars `services.rs` sets on the gatehouse process.
- Adding a new suite: tag the feature `@<service>` with a `Given <service> API is available` background; keep steps that only one service uses under `src/steps/<service>/`, and put anything two services would share in `src/steps/common.rs` (duplicate step patterns across files are an ambiguous-match error at runtime, not a compile error).

[Home](../README.md)
