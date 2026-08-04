# Riveter

Riveter is a Kubernetes manifest tool, powered by Rust and `minijinja` templates. Overlays (per-environment YAML) declare resources by kind; Riveter renders them into full Kubernetes manifests and can apply, diff, delete, or prune them against a cluster with `kubectl`.

See [docs/cli/riveter.md](../../docs/cli/riveter.md) for full documentation.
