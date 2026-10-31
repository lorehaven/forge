# hoist-app
The app crate of Hoist.

## What it is
`hoist-app` is Hoist's screens. The app does no I/O: it asks its host for things (`Request`) and is told what happened (`Msg`), so the same screens run on every host and in tests. Hoist is about app installs and updates from the registry.

## Use cases
- Used by the other `hoist-*` crates; see the layout in the [Hoist README](../README.md).

## Documentation
Full documentation: [services/hoist](../../../docs/services/hoist/index.md)
