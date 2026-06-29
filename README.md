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
