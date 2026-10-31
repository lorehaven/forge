# Warehouse Service
The estate's storage service.

## What it is
`warehouse` is the Docker and Cargo registries, the `.rivet` package registry, file storages and application artifacts in one service. `/v2` and `/token` sit at the server root (registry protocol); everything else is under `BASE_PATH`.

## Use cases
- Push and pull container images and crates.
- Store and browse files.
- Publish `.rivet` packages and app artifacts.

## Documentation
Full documentation: [services/warehouse](../../../docs/services/warehouse/index.md)
