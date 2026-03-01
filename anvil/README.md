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
- `anvil publish [--package <name> | --all]`: Publish package(s) via `cargo publish`.
- `anvil release [--package <name> | --all] [--dry-run]`: Release package(s) based on package tags (`<package>-v<version>`). `--all` uses the union of `[publish].packages` and `[docker.modules.*].packages`. If a package has a prior tag and the current version is already tagged, Anvil bumps patch; if current version is not tagged (manual bump), Anvil publishes current version as-is. If no package tag exists, Anvil creates an initial tag at current version and publishes as-is. When versions are auto-bumped, Anvil runs `cargo build --package <name>`, creates the version update commit, and pushes that commit. Cargo package install runs only for packages listed in `[install].packages`.
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

```toml
[docker]
registry = "ghcr.io/my-org"

[docker.modules.core]
packages = ["service", "worker"]
dockerfile = "Dockerfile"

[docker.modules.core.worker]
dockerfile = "Dockerfile.worker"
image_name = "core-worker"
registries = ["registry.internal/my-org", "backup-registry.internal/my-org"]

[publish]
registry = "forge-registry"
packages = ["service", "worker"]
```

`[docker].registry` is optional only if every package has its own `registries` override.

## License
MIT
