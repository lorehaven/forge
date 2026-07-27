# Forge 🛠️

A modular Rust workspace for development tooling, infrastructure automation, storage services, and LLM orchestration.

## Modules

### `anvil`
Workspace and Docker management CLI.

Features:
- Unified build, lint, and test workflows
- Docker image build and release pipelines
- Multi-project workspace orchestration
- Shared developer automation commands

Requirements:
- `docker`

---

### `riveter`
Kubernetes manifest templating and deployment tooling.

Features:
- Jinja2-based manifest templating
- Interactive REPL workflow
- Kubernetes configuration composition
- Environment-aware rendering pipelines

Requirements:
- `kubectl`

---

### `welder`
Multi-agent LLM execution and orchestration engine.

Features:
- Structured AI workflow execution
- Agent routing and coordination
- Backend abstraction for local inference
- Workflow-driven execution model

Requirements:
- `ollama` or `vllm`

---

### `pulley`
Interactive backup and synchronization tool.

Features:
- TOML-based job configuration
- REPL-driven workflow management
- Incremental synchronization support
- Backup orchestration utilities

Requirements:
- `rsync`

---

### `quench-cli`
Shared terminal UI framework for CLI and REPL applications.

Features:
- Unified terminal interaction model
- Shared REPL infrastructure
- Consistent command UI patterns
- TUI component primitives

---

### `quench-web`
Minimal web UI framework for HTML-based interfaces.

Features:
- Lightweight HTML rendering utilities
- Simple web application scaffolding
- Shared frontend primitives
- Integration-ready web components

---

### `warehouse`
Storage service with REST API and CLI tooling.

Features:
- File storage and retrieval APIs
- CLI-based file management
- Service-oriented architecture
- Local and remote storage workflows

---

### `gatehouse`
Authentication service for the whole estate.

Features:
- One identity store (`auth` schema) shared by every service
- One login page and one realm-wide session cookie
- Realm-wide logout and session revocation
- Relying parties verify tokens locally, with no call on the hot path

Requirements:
- `postgres`

---

### `foundry`
Database initialization job for every Forge service.

Features:
- Single migration catalog of versioned, reusable modules
- Dependency resolution with topological apply order
- Version pinning - install a service at a chosen version
- Runs as a Kubernetes Job or init container

Requirements:
- `postgres`

---

### `conveyor`
CI/CD service for the whole estate.

Features:
- Pipelines defined in-repo as `.conveyor.toml`, read at the commit being built
- Git webhooks in, commit statuses back out
- Postgres-backed run queue, so a restart loses no queued work
- Pluggable executors - local processes, or an isolated Kubernetes Job per job
- `anvil`, `riveter` and `warehouse` as first-class step kinds
- A web UI with live log streaming, and `conveyor-cli` for the terminal

Requirements:
- `postgres`
- `git`

## Prerequisites

- Rust 1.84+ (Edition 2024)
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
cargo run -p welder -- --workflow ./welder/samples/agent.toml
```

## Tests

```bash
cargo test --workspace     # unit and integration tests
fish run.fish test         # the BDD suite, against live services

# the 0.2.0 cutover, end to end, against a restored copy of production
./scripts/rehearse-cutover.fish --database-url postgres://…/prod_copy --yes
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
├── anvil/
├── cli/
│   ├── anvil/
│   ├── pulley/
│   ├── riveter/
│   └── warehouse-cli/
├── libs/
│   ├── quench-cli/
│   └── quench-web/
├── pulley/
├── riveter/
├── warehouse/
└── welder/
```

## License
MIT
