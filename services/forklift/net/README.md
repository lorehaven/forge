# forklift-net
The net crate of Forklift.

## What it is
`forklift-net` is Forklift's talk to the services: gatehouse for the session (the shared `gatehouse-client`) and the data service. Plain blocking calls, to run on a worker thread. Forklift is about the backup of photos, videos and chosen folders.

## Use cases
- Used by the other `forklift-*` crates; see the layout in the [Forklift README](../README.md).

## Documentation
Full documentation: [services/forklift](../../../docs/services/forklift/index.md)
