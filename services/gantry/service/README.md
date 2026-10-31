# Gantry Service
Installs, upgrades, scales and swaps what runs in the cluster.

## What it is
`gantry-service` manages `.rivet` packages on a Kubernetes cluster from a UI: it applies a package's resources, records what it deployed, stops and starts deployments, and upgrades to a new version. It runs `riveter` and `kubectl`, and keeps its state in Postgres and Redis.

## Use cases
- Start and stop a group of deployments (for example inference against training).
- Upgrade a package and see which resources drifted.
- Edit a resource's YAML and apply it.

## Documentation
Full documentation: [services/gantry](../../../docs/services/gantry/index.md)
