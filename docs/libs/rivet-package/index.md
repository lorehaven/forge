---
title: rivet-package
parent: Libraries
has_children: true
permalink: /libs/rivet-package/
---

`rivet-package` (library `rivet_package`) defines the `.rivet` package format: a whole Riveter overlay, versioned and checksummed, stored as a zstd-compressed tar. It parses and validates the manifest `rivet.toml`, builds a deterministic archive, and reads one back with every check a registry needs before it stores the bytes. It touches no network and no database. It exists so that `warehouse-service` (validates a publish), `riveter` (builds and installs packages) and `gantry-service` (reads deployments) share one definition of the format.

## Facts

| | |
|---|---|
| Version | 0.1.6 |
| Edition | 2024 (workspace) |
| License | MIT |
| Library name | `rivet_package` |
| Test harness | `harness = false`; tests run through the `quench-test` macros (`tests/unit.rs`) |
| Unsafe | none (workspace lint) |
| Lints | `#![warn(missing_docs)]` |

## Install

```toml
[dependencies]
rivet-package = { version = "0.1.6", registry = "ennor" }
```

Inside the workspace: `rivet-package = { workspace = true }` (`path = "libs/rivet-package"`, registry `ennor`, version `0.1.0` in `[workspace.dependencies]`). Users in the workspace: `cli/riveter`, `services/gantry/service`, `services/warehouse/service`.

## Features

None. `Cargo.toml` has no `[features]` table.

## Dependencies

| Crate | Kind | Why |
|---|---|---|
| `quench-serde` | normal | `Serialize`/`Deserialize` derives, TOML (`quench_serde::toml`), `quench_serde::Map` for `[meta]` |
| `quench-core` | normal | `Error` derive for `PackageError`, `version::{Version, VersionReq}`, `hex` |
| `quench-crypto` | normal | `hash::sha256` for `SHA256SUMS` |
| `quench-archive` | normal | `tar::{Builder, Reader}` and `zstd::{compress, decompress}` |
| `quench-test` | dev | test harness |

No third-party crates.

## Where to go next

* [Usage](usage.md): build a package, read one, validate a manifest, unpack safely.
* [API reference](api.md): every public item.
* [Package format](format.md): byte-exact layout, `rivet.toml` keys, `SHA256SUMS`.
* [Errors](errors.md): every `PackageError` variant and when it occurs.
* [Internals](internals.md): determinism, limits, safety properties.
* [Testing](testing.md).
* Registry side: `warehouse` service, [rivet registry](../../services/warehouse/api.md#rivets).

## Minimal example

```rust
use rivet_package::{Limits, Manifest, Package, PackageBuilder};
use std::io::Cursor;

fn main() -> Result<(), rivet_package::PackageError> {
    let mut builder = PackageBuilder::new(Manifest::new("forge", "0.4.0"));
    builder.add_file("overlay.yaml", b"env: forge\n".to_vec())?;
    let bytes: Vec<u8> = builder.build()?;          // deterministic tar.zst

    let package = Package::read(Cursor::new(&bytes), &Limits::default())?;
    assert_eq!(package.manifest.package.name, "forge");
    assert!(package.files.contains_key("SHA256SUMS"));
    Ok(())
}
```
