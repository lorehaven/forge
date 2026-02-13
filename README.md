# Build Tools 🛠️

A collection of high-performance development and automation tools for modern software projects. This workspace contains several interconnected modules designed to streamline development, CI/CD, and project maintenance.

## Modules

### [Ferrous 🤖](./ferrous)
An expert multi-purpose assistant and autonomous agent. It runs locally via `llama.cpp` and provides a private, powerful alternative to cloud-based AI coding help. It can discover technologies, plan tasks, edit files, and manage Git.

### [Anvil 🛠️](./anvil)
A workspace-level management tool for Rust projects. It provides unified commands for building, linting, auditing dependencies, and managing Docker release workflows for multiple packages.

### [Riveter 🏗️](./riveter)
A specialized tool for Kubernetes manifest management. It combines the power of Jinja2 templates (via `minijinja`) with environment management and direct `kubectl` integration to simplify infrastructure-as-code tasks.

## Getting Started

Each module can be built and used independently, or you can build everything from the workspace root.

### Prerequisites

- **Rust 1.83+** (edition 2024)
- **llama-server** (required for Ferrous)
- **kubectl** (required for Riveter)
- **docker** (required for Anvil's Docker features)

### Building the Workspace

```bash
cargo build --release
```

## Project Structure

```text
.
├── anvil/      # Workspace & Docker management CLI
├── ferrous/    # AI-driven assistant & autonomous agent
├── riveter/    # Kubernetes manifest templating & management
└── target/     # Build artifacts
```

## License
MIT