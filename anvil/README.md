# Anvil 🛠️

Anvil is a powerful CLI toolset designed for managing multi-package Rust workspaces. It provides a unified interface for common development tasks like building, linting, formatting, and Docker container management.

## Features

- **Project Building**: Unified `build` and `test` commands for individual packages or the entire workspace.
- **Strict Linting**: Pre-configured `lint` command using `cargo clippy` with a comprehensive set of lints (pedantic, nursery, etc.).
- **Workspace Management**:
  - `list`: View all packages in the workspace.
  - `upgrade`: Easily upgrade dependencies to their latest versions.
  - `audit`: Check for security vulnerabilities in dependencies.
  - `machete`: Find and remove unused dependencies.
- **Docker Integration**: Automated Docker operations for workspace packages:
  - `build`: Build Docker images for specific packages.
  - `tag` / `push`: Manage image registry operations.
  - `release`: Combined build, tag, and push workflow.
  - `release-all`: Release all Docker-enabled packages in the workspace.

## Installation

```bash
cd forge/anvil
cargo install --path .
```

## Usage

### General Commands

- `anvil build [--all] [--release] [--package <name>]`: Build packages.
- `anvil test [--all] [--package <name>]`: Run tests.
- `anvil install [--package <name> | --all]`: Install package binary/binaries via `cargo install --path`.
- `anvil lint [--all-targets] [--deny-warnings]`: Run clippy with strict rules.
- `anvil format [--check]`: Format code using `rustfmt`.
- `anvil list [--format <json|text>]`: List workspace members.
- `anvil upgrade [--incompatible]`: Update dependencies.
- `anvil audit`: Run security audit.
- `anvil machete`: Check for unused dependencies.

### Docker Commands

- `anvil docker build <package>`: Build Docker image for a package.
- `anvil docker release <package> <registry>`: Build, tag, and push.
- `anvil docker release-all <registry>`: Release all packages to a registry.

## Configuration

Anvil looks for configuration in the workspace root. It uses these settings to determine Docker registries and package-specific build options.

## License
MIT
