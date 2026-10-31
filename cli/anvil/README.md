# anvil
Workspace build, test and release tool for Cargo projects.

## What it is
`anvil` wraps `cargo` (build, test, clippy, fmt, publish), the dependency checkers (`cargo-audit`, `cargo-deny`, `cargo-machete`, `cargo-semver-checks`) and `docker` with workspace-aware defaults, and automates the release flow for the packages configured in `.anvil.toml` or the files of a `.anvil/` directory (merged).

## Use cases
- Build, test, lint and format a whole workspace with one command.
- Build Docker images for the workspace's services.
- Release packages in dependency order: crates, images and app artifacts.
- Build and publish a documentation book (`anvil docs release`).

## Example
```sh
cd cli/anvil && cargo install --path .
anvil release --all --dry-run   # print the release layers
```

## Release order
`anvil release` publishes in dependency layers: layer 0 has no dependencies among the packages being released, and each later layer depends only on earlier ones, so a package is never published before something it needs. The order does not depend on the order of `[release].packages` and is the same on every run; within a layer, packages go alphabetically. `anvil release --all --dry-run` prints the layers.

Layers follow what `cargo publish` needs in the registry: `dependencies` and `build-dependencies` (including their `[target.*]` forms). A `dev-dependency` only has to exist in the registry already, so it is ordered first only when that is possible: a dev-dependency that would close a cycle (a crate whose tests use a helper that depends on the crate) is ignored for ordering. Whether a package needs releasing at all still counts dev-dependencies, so a change in a test helper releases what uses it.

## Documentation
Full documentation: [cli/anvil](../../docs/cli/anvil/index.md)
