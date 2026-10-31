# rivet-package
The `.rivet` package format.

## What it is
`rivet-package` handles the manifest, deterministic `tar.zst` packing and safe inspection and unpacking of `.rivet` packages: a whole overlay, versioned and checksummed. It touches no network or database, and is shared by `warehouse-service` (which validates publishes) and `riveter` (which builds and installs packages).

## Use cases
- Pack an overlay into a reproducible archive.
- Validate a manifest before publishing.
- Unpack a package without trusting its paths.

## Documentation
Full documentation: [libs/rivet-package](../../docs/libs/rivet-package/index.md)
