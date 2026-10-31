# hoist-net
The net crate of Hoist.

## What it is
`hoist-net` is Hoist's talk to the services: gatehouse for the session (the shared `gatehouse-client`) and the data service. Plain blocking calls, to run on a worker thread. Hoist is about app installs and updates from the registry.

## Use cases
- Used by the other `hoist-*` crates; see the layout in the [Hoist README](../README.md).

## Documentation
Full documentation: [services/hoist](../../../docs/services/hoist/index.md)
