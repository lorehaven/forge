# Forge Documentation

Forge is a modular Rust workspace for development tooling, infrastructure automation, storage, and LLM orchestration. See [Architecture](./ARCHITECTURE.md) for how the pieces below fit together, or jump straight to a page.

## Layout

```text
docs/
├── README.md              # this file
├── ARCHITECTURE.md         # how the services/libs/CLIs fit together
├── cli/                    # cli/* binaries
│   ├── anvil.md
│   ├── conveyor-cli.md
│   ├── foreman.md
│   ├── pulley.md
│   ├── riveter.md
│   ├── toolbox.md
│   ├── warehouse-cli.md
│   └── welder.md
├── docker/                 # docker/* services
│   ├── conveyor-service.md
│   ├── foundry-service.md
│   ├── gatehouse-service.md
│   ├── sage-service.md
│   ├── switchboard-service.md
│   └── warehouse-service.md
├── libs/                   # libs/* shared crates
│   ├── conveyor-pipeline.md
│   ├── quench-auth.md
│   ├── quench-cache.md
│   ├── quench-cli.md
│   ├── quench-client.md
│   ├── quench-config.md
│   ├── quench-db.md
│   ├── quench-starter.md
│   ├── quench-web-components.md
│   └── quench-web.md
├── examples/                # examples/* runnable references
│   ├── basic.md
│   ├── db_example.md
│   └── vllm_cluster_test.md
└── tests/                   # tests/* suites
    └── forge-bdd.md
```

Each page lives at the same path as the crate it documents (e.g. `docker/sage-service` → `docs/docker/sage-service.md`), so the doc to update is always predictable from the code you're touching. Every crate's own `README.md` is a short pointer into its page here — this tree is the actual documentation: descriptions, features, configuration, and CLI/REPL commands or API routes.

## Services (`docker/*`)

Long-running Actix Web services, each with its own Postgres schema, started locally via [Foreman](./cli/foreman.md).

- [Gatehouse Service](./docker/gatehouse-service.md) — the realm's single identity/auth service; issues the tokens every other service verifies
- [Foundry Service](./docker/foundry-service.md) — run-to-completion database migration job for every other service's schema
- [Conveyor Service](./docker/conveyor-service.md) — CI/CD: webhooks in, `.conveyor.toml` pipelines run, commit statuses out
- [Warehouse Service](./docker/warehouse-service.md) — storage: Cargo registry, Docker Registry v2, and plain file storage
- [Switchboard Service](./docker/switchboard-service.md) — model-serving gateway; discovers models, estimates VRAM fit, manages vLLM processes
- [Sage Service](./docker/sage-service.md) — AI chat/workspace app with RAG file upload, built on models Switchboard serves

## CLI Tools (`cli/*`)

- [Foreman](./cli/foreman.md) — brings the local estate up from one `foreman.toml`
- [Anvil](./cli/anvil.md) — workspace build/lint/test/release tool, drives Docker image builds
- [Riveter](./cli/riveter.md) — Kubernetes manifest templating and deployment
- [Conveyor CLI](./cli/conveyor-cli.md) — terminal client for Conveyor
- [Warehouse CLI](./cli/warehouse-cli.md) — terminal client for Warehouse
- [Welder](./cli/welder.md) — multi-agent LLM workflow execution engine
- [Pulley](./cli/pulley.md) — REPL-driven rsync backup/sync tool
- [Toolbox](./cli/toolbox.md) — TUI for managing installed Forge crates

## Shared Libraries (`libs/*`)

- [Quench Auth](./libs/quench-auth.md) — relying-party token/session verification for services behind Gatehouse
- [Quench Starter](./libs/quench-starter.md) — Actix service bootstrap (TLS, base-path scoping, health, DB wiring)
- [Quench Web](./libs/quench-web.md) — dependency-light server-rendered HTML/CSS/JS page builder
- [Quench Web Components](./libs/quench-web-components.md) — higher-level UI builders on top of Quench Web (currently unused workspace-wide)
- [Quench DB](./libs/quench-db.md) — ORM/CRUD abstraction plus the migration catalog engine Foundry drives
- [Quench Cache](./libs/quench-cache.md) — shared in-process/Redis caching layer
- [Quench Client](./libs/quench-client.md) — shared authenticated HTTP client wrappers
- [Quench Config](./libs/quench-config.md) — typed config/env loading helper
- [Quench CLI](./libs/quench-cli.md) — shared terminal UI styling for the CLI tools
- [Conveyor Pipeline](./libs/conveyor-pipeline.md) — `.conveyor.toml` parser/planner shared by Conveyor Service and Conveyor CLI

## Examples & Tests

- [Example: Basic](./examples/basic.md) — minimal Quench Web smoke-test app
- [Example: DB](./examples/db_example.md) — runnable reference for Quench DB's migrations and CRUD
- [Example: vLLM Cluster Test](./examples/vllm_cluster_test.md) — standalone Docker/K8s connectivity smoke test
- [Forge BDD](./tests/forge-bdd.md) — the workspace's single Cucumber BDD suite, run via `foreman test`

## Prerequisites

- Rust 1.85+ (Edition 2024; CI builds on 1.94)
- `docker` (Anvil)
- `kubectl` (Riveter)
- `rsync` (Pulley)
- `ollama` or `vllm` (Welder)

## Build

```bash
cargo build --release
```

### Quench UI Smoke Tests

```bash
# quench web example
cargo run -p quench-example-basic

# anvil cli
cargo run -p anvil -- --help

# riveter repl
cargo run -p riveter -- repl

# pulley repl
cargo run -p pulley

# warehouse cli
cargo run -p warehouse-cli -- --help

# toolbox tui
cargo run -p forge-toolbox

# welder workflow repl
cargo run -p welder -- --workflow ./cli/welder/samples/agent.toml
```

## Local development

`foreman` brings the estate up on localhost: postgres and redis in docker,
foundry over the database, then the services from `target/debug`. What it
starts, on which ports, and with which environment is all in `foreman.toml` at
the repository root.

```bash
cargo install --path cli/foreman

foreman                       # the whole estate
foreman start conveyor        # only conveyor, and what it needs
foreman repl                  # pick services interactively, then `up`
foreman status                # what is up, and on which port
foreman logs conveyor         # follow one service's log
foreman stop [all]            # services, or services and containers too
foreman list                  # what foreman.toml defines
foreman env sage               # the environment one service would start with
```

Naming services starts a subset: only those packages are built, and foundry
installs only their schemas. Working on conveyor needs gatehouse for the realm
but not sage's model launch or switchboard's GPU, and `start conveyor` pulls
gatehouse in on its own — a subset is never a half-wired estate.

## Tests

```bash
cargo test --workspace     # unit and integration tests
foreman test               # the BDD suite, against live services
```

The Redis-backed cache tests are skipped unless `CACHE_TEST_REDIS_URL` is set;
the cluster ones want `CACHE_TEST_REDIS_CLUSTER_URL` with comma-separated seeds.

No crate keeps tests in `src/`. Each one has a `tests/unit.rs` entry point that
declares the test modules, and the modules themselves live in `tests/unit/`,
named after the file they cover:

```text
docker/sage-service/
├── src/tools/parser.rs
└── tests/
    ├── unit.rs                       #[path = "unit/tools_parser_tests.rs"] mod …
    └── unit/tools_parser_tests.rs    use sage_service::tools::parser::*;
```

Because a test in `tests/` is a separate crate, it can only reach the public
API. The services are therefore `[lib]` + `[[bin]]`: the lib holds the modules
and `main.rs` is a thin entry point. Where a genuinely internal helper is worth
testing, it is `#[doc(hidden)] pub` in the libraries and plain `pub` in the
services.

## Project Structure

```text
.
├── cli/           # binaries: anvil, conveyor-cli, foreman, pulley, riveter,
│                  #           toolbox, warehouse-cli, welder
├── docker/        # services: conveyor, foundry, gatehouse, sage,
│                  #           switchboard, warehouse (each its own image)
├── libs/          # shared crates: conveyor-pipeline, quench-*
├── examples/      # runnable references: basic, db_example, vllm_cluster_test
├── tests/         # forge-bdd, the cross-service Cucumber suite
├── ci/            # CI-only images (e.g. rust-builder)
├── docs/          # this documentation tree
└── foreman.toml   # what foreman starts locally, on which ports
```
