# forklift-app
The app crate of Forklift.

## What it is
`forklift-app` is Forklift's screens. The app does no I/O: it asks its host for things (`Request`) and is told what happened (`Msg`), so the same screens run on every host and in tests. Forklift is about the backup of photos, videos and chosen folders.

## Use cases
- Used by the other `forklift-*` crates; see the layout in the [Forklift README](../README.md).

## Documentation
Full documentation: [services/forklift](../../../docs/services/forklift/index.md)
