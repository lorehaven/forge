# Anvil

Anvil is the workspace build tool for the Forge Cargo workspace. It wraps `cargo build`/`test`/`clippy`/`fmt` with workspace-aware defaults, drives Docker image builds for the workspace's services, and automates the release flow (version bump, commit, `cargo publish`/`cargo install`, or Docker build+tag+push) for packages configured in `.anvil.toml`. It exists so that every crate and service in the workspace is built, linted, and released the same way instead of each one growing its own scripts.

## Features

- **Build/test/clean**: workspace-wide or single-package `build`, `test`, and `clean`, mirroring `cargo`'s own flags (`--all`, `--all-features`, `--release`, `-p/--package`).
- **Faster test runner**: `nextest` runs the same package/test-name selection through `cargo-nextest` — parallel-by-default, per-test timeouts, non-interleaved output. Doesn't cover doctests or `forge-bdd`'s cucumber suite (a binary target, not a `#[test]` harness).
- **Strict linting**: `lint` runs `cargo clippy` with `--all-targets`, `--all-features`, and warnings-as-errors all defaulting to on.
- **Formatting**: `format [--check]` wraps `cargo fmt`.
- **Workspace introspection**: `list [--format names|json]` enumerates workspace packages.
- **Dependency maintenance**: `upgrade [--incompatible]`, `audit` (RustSec advisories via `cargo-audit`), `machete` (unused-dependency detection via `cargo-machete`), `deny` (licenses, banned/duplicate crates, and registry sources via `cargo-deny`, configured in `deny.toml`).
- **API stability**: `semver-check -p <name> [--baseline-rev <rev>]` diffs a library crate's public API with `cargo-semver-checks` against a git revision (default: the commit before its last `Cargo.toml` version bump) rather than a registry version, since the crates this workspace publishes live on the private `ennor` registry that `cargo-semver-checks` can't query directly.
- **Run/serve mode**: `run` builds and runs a package binary; with `--serve` it watches for file changes and rebuilds/restarts, with interactive hotkeys.
- **Docker integration**: builds, tags, pushes, and releases Docker images for packages declared under `[docker.modules.*]` in `.anvil.toml`, including multi-registry pushes and per-package `build_args`.
- **Release automation**: `release` bumps a package's patch version, commits, and then either runs the Docker release flow or `cargo publish` (+ `cargo install` when the package is also listed under `[install].packages`).

## Requirements

- `cargo` and the standard Rust toolchain (`clippy`, `rustfmt` components for `lint`/`format`).
- `cargo-machete` on `PATH` for `anvil machete`.
- `cargo-deny` on `PATH` for `anvil deny`.
- `cargo-nextest` on `PATH` for `anvil nextest`.
- `cargo-semver-checks` on `PATH` for `anvil semver-check`.
- `docker` on `PATH` for any `anvil docker ...` subcommand.
- A `.anvil.toml` file at the workspace root for Docker/install/release package configuration (Anvil falls back to an empty config with a warning if it's missing or fails to parse).

## Installation

```bash
cd forge/cli/anvil
cargo install --path .
```

## Usage

### Workspace and Cargo commands

```bash
anvil build [--all] [--all-features] [--release] [-p|--package <name>]
anvil clean
anvil test [--all] [-p|--package <name>] [<test_name>] [--ignored] [--list]
anvil nextest [--all] [-p|--package <name>] [<test_name>] [--ignored]
anvil run [-p|--package <name>] [--serve] [--watch-interval-ms <ms>]
anvil lint [--all-targets] [--all-features] [--deny-warnings]
anvil format [--check]
anvil list [--format names|json]
anvil upgrade [--incompatible]
anvil audit
anvil machete
anvil deny
anvil semver-check -p <name> [--baseline-rev <rev>]
```

In `run --serve` mode: `r` rebuilds immediately, `R` toggles auto-rebuild-on-change, `q`/`Q`/`e`/`E` quit.

### Release and distribution

```bash
anvil install [--all | -p|--package <name>]
anvil release [--all | -p|--package <name>] [--dry-run]
```

`anvil release --all` releases every package listed in `[release].packages` plus any package under a `[docker.modules.*]` block. `--dry-run` previews the plan without touching files or publishing anything.

### Docker

```bash
anvil docker build   -p|--package <name>
anvil docker tag     -p|--package <name>
anvil docker push    -p|--package <name>
anvil docker release -p|--package <name>   # build + tag + push
anvil docker build-all
anvil docker release-all
```

## Configuration

Anvil reads `.anvil.toml` from the workspace root:

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

[install]
packages = ["service", "worker"]

[release]
registry = "forge-registry"
packages = ["service", "worker"]
```

`[docker].registry` is only optional when every package sets its own `registries` override. Every Docker build always receives `PROJECT_NAME` and `RESOURCES_PATH` build args derived from the module/package name; `build_args` entries are appended after those, so a package can override `RESOURCES_PATH` if it genuinely needs a different one. This lets one parameterized Dockerfile serve many packages — prefer `build_args` over forking a Dockerfile for a one-line difference.

[Home](../README.md)
