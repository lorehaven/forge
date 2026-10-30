---
title: Usage
parent: rivet-package
grand_parent: Libraries
nav_order: 1
---

Worked examples, from the common to the advanced. All code uses the public API only; the same flows are exercised in `tests/unit/archive_tests.rs` and `manifest_tests.rs`.

## 1. Parse and validate a manifest

Goal: read a `rivet.toml` and fail on any rule violation.

```rust
use rivet_package::Manifest;

let m = Manifest::parse("[package]\nname = \"forge\"\nversion = \"0.4.0+b123\"\n")?;
assert_eq!(m.package.name, "forge");
assert_eq!(m.version()?.build.as_str(), "b123");
assert!(m.requires.is_empty());
# Ok::<(), rivet_package::PackageError>(())
```

Effect: `parse` deserialises, then calls `validate`. Pitfalls: a typo such as `namspace` under `[package]` is an error (`deny_unknown_fields` on `[package]`, `[requires]`, `[[deployment]]` and `[[deployment.also_stops]]`); unknown top-level tables are *not* rejected. `version = "1.0"` fails (not semver).

## 2. Build a package in memory or as bytes

```rust
use rivet_package::{Manifest, PackageBuilder};

let mut b = PackageBuilder::new(Manifest::new("forge", "0.4.0"));
b.add_file("overlay.yaml", b"env: forge\n".to_vec())?;
b.add_file("templates/a.yaml.j2", b"kind: A\n".to_vec())?;
let package = b.finish()?;        // Package { manifest, files } with rivet.toml and SHA256SUMS added
let bytes = package.to_bytes()?;  // tar.zst
# Ok::<(), rivet_package::PackageError>(())
```

`build()` is `finish()?.to_bytes()`. Insertion order does not matter; identical input gives byte-identical output. Pitfalls: `rivet.toml` and `SHA256SUMS` are reserved and rejected by `add_file`; `overlay.yaml` is required at `finish`; adding a name twice is `PackageError::Archive("`x` added twice")`.

## 3. Read and verify a package from a stream

```rust
use rivet_package::{Limits, Package};

let file = std::fs::File::open("forge-0.4.0.rivet")?;
let package = Package::read(file, &Limits::default())?;
println!("{} {}", package.manifest.package.name, package.manifest.package.version);
# Ok::<(), Box<dyn std::error::Error>>(())
```

The whole stream is read into memory, decompressed with a cap, every entry validated, the manifest parsed and `SHA256SUMS` compared with the contents. See [internals](internals.md) for the order of checks.

## 4. Tighter limits for an untrusted source

```rust
use rivet_package::{Limits, Package};
let limits = Limits { max_entries: 200, max_file_bytes: 1 << 20, max_total_bytes: 8 << 20 };
let result = Package::read(&bytes[..], &limits);  // Err(PackageError::Limit(..)) when exceeded
```

## 5. Unpack into a fresh directory

```rust
package.write_to(std::path::Path::new("/tmp/overlay-out"))?;
```

Every file is created with `create_new`, so an existing file (or a planted symlink at the target) makes the call fail with `PackageError::Io` instead of overwriting. Parent directories are created. The directory itself need not exist. A partially written tree is not cleaned up on failure.

## 6. Deployments and requirements

```toml
[package]
name = "ml"
version = "1.2.0"

[requires]
riveter = ">=0.3"
packages = ["postgres >=1", "redis"]

[[deployment]]
name = "inference"
resources = ["deployment/sage", "statefulset/models"]
default = "running"
conflicts_with = ["training"]

[[deployment.also_stops]]
selector = "app=vllm"
namespaces = ["ml"]
```

`rivet_package::manifest::parse_package_requirement("postgres >=1")` returns `("postgres", VersionReq)`; without a constraint the requirement is `VersionReq::STAR`.

*Unverified:* the snippets on this page were written from the sources and the crate's tests; they were not compiled or run here.
