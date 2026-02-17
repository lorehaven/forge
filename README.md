# Build Tools 🛠️

A collection of high-performance development and automation tools for modern software projects. This workspace contains several interconnected modules designed to streamline development, CI/CD, and project maintenance.

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
├── riveter/    # Kubernetes manifest templating & management
├── welder/     # AI-driven agent system framework
└── target/     # Build artifacts
```

## License
MIT