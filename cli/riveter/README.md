# riveter
Kubernetes manifest renderer and applier, with `.rivet` packages.

## What it is
`riveter` renders Kubernetes manifests from per-environment overlays (`overlays/<env>/overlay.yaml`, minijinja templates) and applies, diffs, deletes and prunes them with `kubectl`. It also packs an overlay into a versioned `.rivet` package and publishes it to, and installs it from, Warehouse. With no subcommand it starts a REPL.

## Use cases
- Render and validate an environment's manifests without a cluster.
- Apply, diff or prune one or several environments.
- Pack, publish and pull `.rivet` packages.
- Manage encrypted secrets for an overlay.

## Example
```sh
riveter -e prod render
riveter -e prod diff
riveter -e prod apply
```

## Documentation
Full documentation: [cli/riveter](../../docs/cli/riveter/index.md)
