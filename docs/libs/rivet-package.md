# Rivet Package

`rivet-package` (crate `rivet_package`) defines the `.rivet` package format: a whole Riveter overlay, versioned and checksummed, stored as a `tar.zst`. It touches no network or database — it parses and validates the manifest, builds a deterministic archive, and reads one back with every check a registry needs before it stores the bytes. It exists as its own crate so `warehouse-service` (which validates a publish) and `riveter` (which builds and installs packages) share one definition of the format rather than two that drift apart; its own README is a short pointer to this page.

## Format

`<name>-<version>.rivet`, a zstd-compressed tar of regular files at the archive root. The root is the overlay directory: its files sit at their paths relative to it.

```
rivet.toml          # the manifest (generated from the overlay's own on pack)
overlay.yaml        # the overlay, unrendered - required
base.yaml.j2 ...    # whatever the overlay includes, at its original relative path
values.yaml         # optional defaults for the overlay's ${VARS} (or values.toml)
secrets.yaml        # optional encrypted values, safe to ship: NAME: ENC[age,...]
.env.example        # optional documentation of what to supply
SHA256SUMS          # `<hex>  <path>` per file, covering everything except itself
```

The format itself only requires `rivet.toml`, `overlay.yaml` and `SHA256SUMS`; what else a package holds, and that its name matches the overlay directory, is Riveter's concern ([packages](../cli/riveter.md#packages)).

`rivet.toml`:

```toml
[package]
name = "forge"            # DNS-1123 label, the registry key
version = "0.4.0+b123"    # semver; build metadata allowed
description = "one line"  # optional
namespace = "forge"       # optional, DNS-1123

[requires]                # optional
riveter = ">=0.3"         # a semver requirement on the installing riveter
packages = ["postgres >=1", "redis"]

[meta]                    # free-form; carried and shown, never interpreted

[[deployment]]            # optional: things Gantry can stop and start on their own
name = "inference"        # DNS-1123, unique across every package Gantry manages
description = "one line"
resources = ["deployment/sage", "deployment/switchboard"]   # stop order; start goes back up it
default = "running"       # or "stopped"; what it is until someone says otherwise
conflicts_with = ["training"]   # must not run alongside; starting this plans their stop first
[[deployment.also_stops]]       # pods nothing owns that hold what it holds
selector = "app=vllm"           # as `kubectl -l`; deleted after the scale-down and waited on
namespaces = ["ml"]             # empty: wherever Gantry may act
```

Unknown keys under `[package]`, `[requires]` and `[[deployment]]` are errors, so a typo (`namspace`) surfaces instead of being ignored. A deployment's resources must be `deployment/<name>` or `statefulset/<name>` (the only kinds that scale), names are unique within a manifest, and one cannot conflict with itself; `riveter pack` additionally checks that every named workload is in the overlay. A package that declares no `[[deployment]]` is, to Gantry, one deployment named after the package.

## Public API

- `Manifest` / `PackageMeta` / `Requires` / `Deployment` / `AlsoStops` / `DefaultState` — `Manifest::parse` and `to_toml` validate (name rule, semver version, requirement syntax, one-line description); `Manifest::version()` gives the parsed `semver::Version`. `manifest::is_valid_name` and `parse_package_requirement` are the building blocks.
- `PackageBuilder` — `add_file(path, bytes)` for the overlay's own files (`rivet.toml` and `SHA256SUMS` are reserved and generated; `overlay.yaml` is required), then `finish()` for an in-memory `Package` or `build()` for the bytes.
- `Package::read(reader, &Limits)` — decode and fully validate, or fail with a `PackageError`. `Package::to_bytes()` serialises; `Package::write_to(dir)` materialises files with `create_new`, so nothing existing is overwritten or followed.
- `Limits` — `max_entries` (2000), `max_file_bytes` (16MiB), `max_total_bytes` (128MiB), all decompressed sizes.
- `path::validate` — the entry-path rule. `sha256_hex`, `file_name(name, version)` and the file-name constants (`MANIFEST_FILE`, `OVERLAY_FILE`, `SUMS_FILE`, `VALUES_FILE`, `EXTENSION`).

## Guarantees

**Deterministic output.** Entries are written sorted, with mode `0644`, uid/gid `0` and mtime `0`, so identical input gives an identical archive, and insertion order doesn't matter. The digest is stable for a given zstd library version; it isn't promised across upgrades of that library, so a registry should record the digest of the bytes it stored rather than recompute one.

**Safe reading.** Reading never extracts through `tar`: entries are validated and held in memory, so an archive has no way to choose where a byte lands. A path must be relative, UTF-8, free of `.`/`..`/empty components, backslashes and control bytes, and within length limits. Only regular files are accepted — symlinks, hard links, devices and FIFOs are refused, as are duplicate entries. The size limits are enforced against the *declared* entry sizes and, as a backstop, against the decoded stream itself, so a small archive that expands enormously is stopped. Every file must appear in `SHA256SUMS` with a matching digest, and `SHA256SUMS` may list nothing the archive lacks.

## Testing

`libs/rivet-package/tests/unit.rs` wires `archive_tests.rs`, `manifest_tests.rs` and `path_tests.rs`: round trips, byte-identical rebuilds regardless of insertion order, traversal / absolute / link / device rejection (crafted with raw tar headers, since the tar crate's own writer refuses to produce them), missing required files, checksum mismatches and smuggled or phantom files, duplicate entries, each limit, and a highly compressible archive that must be stopped by the decoded-size cap.
