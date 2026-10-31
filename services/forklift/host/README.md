# forklift-host
The host crate of Forklift.

## What it is
`forklift-host` is Forklift's controller the hosts share, over a `Platform` trait: changes show at once and are undone if refused. It touches the device only through `Platform`, so it runs the same under a test. Forklift is about the backup of photos, videos and chosen folders.

## Use cases
- Used by the other `forklift-*` crates; see the layout in the [Forklift README](../README.md).

## Documentation
Full documentation: [services/forklift](../../../docs/services/forklift/index.md)
