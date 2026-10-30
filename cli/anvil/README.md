# Anvil

Anvil is the workspace build tool for the Forge Cargo workspace — it wraps `cargo build`/`test`/`clippy`/`fmt` with workspace-aware defaults, drives Docker image builds for the workspace's services, and automates the release flow for packages configured in `.anvil.toml` or the files of a `.anvil/` directory (merged; see `docs/cli/anvil.md`).

See [docs/cli/anvil](../../docs/cli/anvil/index.md) for full documentation.

## Release order

`anvil release` publishes in dependency layers: layer 0 has no dependencies among the packages being released, and each later layer depends only on earlier ones, so a package is never published before something it needs. The order does not depend on the order of `[release].packages` and is the same on every run; within a layer, packages go alphabetically. `anvil release --all --dry-run` prints the layers.

Layers follow what `cargo publish` needs in the registry: `dependencies` and `build-dependencies` (including their `[target.*]` forms). A `dev-dependency` only has to exist in the registry already, so it is ordered first only when that is possible — a dev-dependency that would close a cycle (a crate whose tests use a helper that depends on the crate) is ignored for ordering. Whether a package needs releasing at all still counts dev-dependencies, so a change in a test helper releases what uses it.
