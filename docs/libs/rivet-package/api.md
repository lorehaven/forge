---
title: API reference
parent: rivet-package
grand_parent: Libraries
nav_order: 2
---

Every public item, module by module in source order (`src/lib.rs`, `archive.rs`, `error.rs`, `manifest.rs`, `path.rs`).

## Crate root (`lib.rs`)

Modules: `pub mod archive; pub mod error; pub mod manifest; pub mod path;`.

Re-exports:

```rust
pub use archive::{Limits, Package, PackageBuilder};
pub use error::PackageError;
pub use manifest::{AlsoStops, DefaultState, Deployment, Manifest, PackageMeta, Requires};
```

Constants:

| Item | Value | Meaning |
|---|---|---|
| `pub const EXTENSION: &str` | `"rivet"` | extension of a package file, without the dot |
| `pub const MANIFEST_FILE: &str` | `"rivet.toml"` | the manifest's fixed name inside the archive |
| `pub const OVERLAY_FILE: &str` | `"overlay.yaml"` | overlay entry point; every package must carry it |
| `pub const SUMS_FILE: &str` | `"SHA256SUMS"` | per-file digests; covers every file except itself |
| `pub const VALUES_FILE: &str` | `"values.toml"` | optional default values, overridable at install time. Declared only; this crate never reads or requires it |

Functions:

```rust
#[must_use] pub fn sha256_hex(data: &[u8]) -> String
```
Lower-case hex SHA-256 (64 characters) of `data`.

```rust
#[must_use] pub fn file_name(name: &str, version: &str) -> String
```
`format!("{name}-{version}.{EXTENSION}")`, for example `file_name("forge", "0.4.0+b123")` is `forge-0.4.0+b123.rivet`. Performs no validation.

## `archive`

### `Limits`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits { pub max_entries: usize, pub max_file_bytes: u64, pub max_total_bytes: u64 }
```

| Field | Default | Meaning |
|---|---|---|
| `max_entries` | `2000` | number of files (directory entries are skipped and not counted) |
| `max_file_bytes` | `16 * 1024 * 1024` | decompressed size of any one file |
| `max_total_bytes` | `128 * 1024 * 1024` | sum of all decompressed file sizes |

`impl Default for Limits` yields the values above.

### `Package`

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct Package { pub manifest: Manifest, pub files: BTreeMap<String, Vec<u8>> }
```

`files` holds every file, including `rivet.toml` and `SHA256SUMS`, keyed by path. `Package` is `Send + Sync` (owned data only).

```rust
pub fn Package::read(reader: impl Read, limits: &Limits) -> Result<Package, PackageError>
```
Reads the whole stream, zstd-decompresses it (cap: `max_total_bytes + (max_entries + 2) * 1024` bytes of tar), walks the tar entries and runs the structural checks. Errors: `Archive`, `Limit`, `UnsafePath`, `Missing`, `Manifest`, `Checksum`, `Io`. Order of checks is in [internals](internals.md#read-algorithm).

```rust
pub fn Package::write_to(&self, dir: &Path) -> Result<(), PackageError>
```
Writes each file below `dir` (parent directories created), opening with `create_new`. Re-validates each name with `path::validate`. Fails with `Io` if a target exists.

```rust
pub fn Package::to_bytes(&self) -> Result<Vec<u8>, PackageError>
```
Serialises `files` (sorted by key) into a tar and zstd-compresses it. Does not re-validate; byte-identical for identical `files`.

### `PackageBuilder`

```rust
#[derive(Debug)] pub struct PackageBuilder { /* private */ }
```

```rust
#[must_use] pub const fn PackageBuilder::new(manifest: Manifest) -> Self
```
The manifest is validated at `finish`, not here.

```rust
pub fn add_file(&mut self, name: impl Into<String>, data: Vec<u8>) -> Result<&mut Self, PackageError>
```
Validates the path (`path::validate`), rejects `rivet.toml` and `SHA256SUMS` (`UnsafePath(name, "reserved file name")`) and duplicates (`Archive("`name` added twice")`).

```rust
pub fn finish(self) -> Result<Package, PackageError>
```
Requires `overlay.yaml` (`Missing("overlay.yaml")`), adds `rivet.toml` from `manifest.to_toml()?` (validates), then `SHA256SUMS` with one line `"{sha256_hex}  {path}\n"` per file in sorted order (including `rivet.toml`, excluding `SHA256SUMS`).

```rust
pub fn build(self) -> Result<Vec<u8>, PackageError>
```
`self.finish()?.to_bytes()`.

## `error`

### `PackageError`

```rust
#[derive(Debug, quench_core::Error)]
pub enum PackageError {
    Manifest(String), Archive(String), UnsafePath(String, &'static str),
    Limit(String), Missing(&'static str), Checksum(String), Io(std::io::Error),
}
```
Texts and causes: [errors](errors.md).

## `manifest`

### `Manifest`

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub package: PackageMeta,
    #[serde(default, skip_serializing_if = "Requires::is_empty")] pub requires: Requires,
    #[serde(default, skip_serializing_if = "quench_serde::Map::is_empty")] pub meta: quench_serde::Map,
    #[serde(default, rename = "deployment", skip_serializing_if = "Vec::is_empty")] pub deployments: Vec<Deployment>,
}
```
Methods:

| Signature | Behaviour |
|---|---|
| `#[must_use] pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self` | only the required fields; no validation |
| `pub fn parse(source: &str) -> Result<Self, PackageError>` | TOML to `Manifest`, then `validate`; `Manifest` error with the parser message otherwise |
| `pub fn to_toml(&self) -> Result<String, PackageError>` | validates, then serialises with `quench_serde::toml::to_string` |
| `pub fn version(&self) -> Result<quench_core::version::Version, PackageError>` | parsed `package.version`; error text ``version `V` is not semver: ...`` |
| `pub fn validate(&self) -> Result<(), PackageError>` | every rule in [format](format.md#validation-rules) |

### `PackageMeta`

`#[serde(deny_unknown_fields)]`. Fields: `name: String`, `version: String`, `description: Option<String>` (default none), `namespace: Option<String>` (default none). Optional fields are skipped when serialising if `None`.

### `Requires`

`#[derive(Default)]`, `#[serde(deny_unknown_fields)]`. Fields: `riveter: Option<String>`, `packages: Vec<String>` (default empty, skipped when empty).

```rust
#[must_use] pub const fn Requires::is_empty(&self) -> bool   // riveter.is_none() && packages.is_empty()
```

### `Deployment`

`#[serde(deny_unknown_fields)]`, derives `Debug, Clone, PartialEq, Eq, Serialize, Deserialize`.

| Field | Type | Default | Meaning |
|---|---|---|---|
| `name` | `String` | required | DNS-1123 label, unique across every package Gantry manages |
| `description` | `Option<String>` | none | one line for UIs; the one-line/500 limit is **not** checked for deployments |
| `resources` | `Vec<String>` | required | `kind/name` in stop order |
| `default` | `DefaultState` | `Running` | state when nobody says otherwise |
| `conflicts_with` | `Vec<String>` | empty | deployments that must not run concurrently |
| `also_stops` | `Vec<AlsoStops>` | empty | unowned pods to delete after scale-down |

### `DefaultState`

`#[derive(Default, Copy, ...)] #[serde(rename_all = "lowercase")] pub enum DefaultState { Running /* default */, Stopped }`. TOML strings `"running"`, `"stopped"`.

### `AlsoStops`

`#[serde(deny_unknown_fields)]`; `selector: String` (required), `namespaces: Vec<String>` (default empty, meaning every namespace Gantry may touch).

### Free functions

```rust
#[must_use] pub fn is_valid_name(name: &str) -> bool
```
DNS-1123 label: 1 to 63 bytes, lower-case ASCII letters, digits and `-`, first and last a letter or digit.

```rust
pub fn parse_package_requirement(entry: &str) -> Result<(String, quench_core::version::VersionReq), PackageError>
```
Trims, splits at the first whitespace into `name` and the rest. Name must satisfy `is_valid_name`; an empty rest is `VersionReq::STAR`. Errors: ``requires.packages entry `E`: `N` is not a valid package name`` or ``requires.packages entry `E`: {parse error}``.

## `path`

```rust
pub const MAX_PATH_LEN: usize = 512;
pub fn validate(path: &str) -> Result<(), PackageError>
```
Checks, in this order: empty (`empty`); `len() > 512` bytes (`too long`); leading `/` (`absolute`); any `\`, NUL or control character (`contains a backslash or control character`); then per `/` component: empty (`empty component`), `.` or `..` (`relative component`), longer than 255 bytes (`component too long`). Failure is `PackageError::UnsafePath(path, reason)`. A trailing slash therefore gives `empty component`. The 255-byte component limit is a private constant.
