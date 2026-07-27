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

### Command Help

#### Workspace and Cargo Commands

- `anvil build [--all] [--all-features] [--release] [-p|--package <name>]`
- `anvil clean`
- `anvil test [--all] [-p|--package <name>] [<test_name>] [--ignored] [--list]`
- `anvil run [-p|--package <name>] [--serve] [--watch-interval-ms <ms>]`
  - `--serve` starts watch/rebuild mode.
  - Hotkeys: `r` rebuild now, `R` toggle auto-rebuild, `q|Q|e|E` quit.
- `anvil lint [--all-targets] [--all-features] [--deny-warnings]`
  - Defaults are `true` for these three flags.
- `anvil format [--check]`
- `anvil list [--format names|json]` (default: `names`)
- `anvil upgrade [--incompatible]`
- `anvil audit`
- `anvil machete`

#### Release and Distribution Commands

- `anvil install [--all | -p|--package <name>]`
- `anvil publish [--all | -p|--package <name>]`
- `anvil release [--all | -p|--package <name>] [--dry-run]`
  - `--all` uses packages configured in `[publish].packages` and Docker module package lists.
  - In non-dry runs, auto-bumps and tagging can trigger build/commit/push flow before publish/install.

#### Docker Commands

- `anvil docker build -p|--package <name>`: build one image
- `anvil docker tag -p|--package <name>`: tag one image
- `anvil docker push -p|--package <name>`: push one image
- `anvil docker release -p|--package <name>`: build + tag + push
- `anvil docker build-all`: build all configured Docker packages
- `anvil docker release-all`: build + tag + push all configured Docker packages

## Configuration

Anvil looks for configuration in the workspace root. It uses these settings to determine Docker registries and package-specific build options.

```toml
[docker]
registry = "ghcr.io/my-org"

[docker.modules.core]
packages = ["service", "worker"]
dockerfile = "Dockerfile"

[docker.modules.core.worker]
image_name = "core-worker"
registries = ["registry.internal/my-org", "backup-registry.internal/my-org"]
build_args = { RUNTIME_PACKAGES = "git", RUN_AS = "999:999" }

[publish]
registry = "forge-registry"
packages = ["service", "worker"]
```

`[docker].registry` is optional only if every package has its own `registries` override.

### Build arguments

Anvil always passes `PROJECT_NAME` and `RESOURCES_PATH`, derived from the module
and package names. A package that needs more declares `build_args`, and they are
passed after the derived ones - so a package that genuinely needs a different
`RESOURCES_PATH` can override it too.

This is what lets one parameterised Dockerfile serve packages that differ in
small ways. Reach for `build_args` before `dockerfile`: a forked Dockerfile that
exists for the sake of one line stops tracking the original the moment the
original changes.

## License
MIT
