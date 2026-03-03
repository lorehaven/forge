# Forge 🛠️

A collection of high-performance development and automation tools for modern software projects.
This workspace contains several modules designed to streamline development, CI/CD, and project maintenance.

## Modules

### Development Tools

- **anvil** - Workspace and Docker management CLI with unified build, lint, test, and container release workflows
- **riveter** - Kubernetes manifest templating and management with Jinja2 templates and interactive REPL
- **welder** - Multi-agent LLM execution engine for building, routing, and coordinating structured AI workflows

### Backup/Sync Tools

- **pulley** - Interactive REPL-based backup tool with TOML configuration and job management

### Frameworks & Services

- **quench-cli** - Unified terminal UI module with CLI and REPL modes
- **quench-web** - Simple web UI framework library for HTML-based interfaces
- **warehouse** - Storage service with REST API server and CLI tool for file management

## Getting Started

Each module can be built and used independently, or you can build everything from the workspace root.

### Prerequisites

- **Rust 1.84+** (edition 2024)
- **ollama** (required for Welder)
- **kubectl** (required for Riveter)
- **docker** (required for Anvil's Docker features)
- **rsync** (required for backup tools)

### Building the Workspace

```bash
cargo build --release
```

### Quench UI Smoke Tests

```bash
# web shell + components
cargo run -p quench-example-basic

# anvil cli (shared quench-cli terminal ui)
cargo run -p anvil -- --help

# riveter repl (shared quench-cli terminal ui)
cargo run -p riveter -- repl

# pulley repl (shared quench-cli terminal ui)
cargo run -p pulley

# warehouse cli (shared quench-cli terminal ui)
cargo run -p warehouse-cli -- --help

# toolbox tui (shared quench-cli status hook + toolbox layout)
cargo run -p forge-toolbox

# welder repl (shared quench-cli terminal constants + layout)
cargo run -p welder -- --workflow ./welder/samples/agent.toml
```

## Project Structure

```text
.
├── anvil/              # Workspace & Docker management CLI
├── pulley/             # Interactive backup tool (TOML config, REPL)
├── quench/             # Quench modules (web + cli)
├── riveter/            # Kubernetes manifest templating
├── warehouse/          # Storage service (API + CLI)
└── welder/             # Multi-agent LLM framework
```

## License
MIT
