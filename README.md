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

- **quench** - Simple web UI framework library for building HTML-based interfaces
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

## Project Structure

```text
.
├── anvil/              # Workspace & Docker management CLI
├── pulley/             # Interactive backup tool (TOML config, REPL)
├── quench/             # Web UI framework
├── riveter/            # Kubernetes manifest templating
├── warehouse/          # Storage service (API + CLI)
└── welder/             # Multi-agent LLM framework
```

## License
MIT