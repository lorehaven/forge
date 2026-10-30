---
title: Errors
parent: rivet-package
grand_parent: Libraries
nav_order: 4
---

All fallible functions return `Result<_, PackageError>`. `PackageError` derives `Debug` and `quench_core::Error`; `Display` texts below are exact. Only `Io` has a `From` conversion (`#[from] std::io::Error`).

| Variant | Display text | Occurs when |
|---|---|---|
| `Manifest(String)` | `invalid manifest: {0}` | TOML syntax or type errors; any rule of `Manifest::validate` (see [format](format.md#validation-rules)); `rivet.toml` is not UTF-8 (`rivet.toml is not UTF-8`); `Manifest::to_toml` serialisation failure |
| `Archive(String)` | `invalid archive: {0}` | input is not zstd (`not a zstd stream: ...`); tar parse error; a path appears twice (`` `x` appears twice``); builder: `` `x` added twice`` |
| `UnsafePath(String, &'static str)` | ``unsafe path `{0}`: {1}`` | `path::validate` fails (reasons: `empty`, `too long`, `absolute`, `contains a backslash or control character`, `empty component`, `relative component`, `component too long`); non-regular tar entry (`only regular files are allowed (no links or devices)`); builder: `reserved file name` |
| `Limit(String)` | `archive exceeds limits: {0}` | decoded stream over the cap; more than `max_entries` files (`more than N files`); a file over `max_file_bytes` (`` `x` is larger than N bytes``); sum over `max_total_bytes` (`contents exceed N bytes`) |
| `Missing(&'static str)` | ``package is missing `{0}` `` | `rivet.toml`, `overlay.yaml` or `SHA256SUMS` absent on read (checked in that order); `overlay.yaml` absent at `PackageBuilder::finish` |
| `Checksum(String)` | `checksum mismatch: {0}` | `SHA256SUMS` not UTF-8; malformed line; file listed twice; a file not listed (`` `x` is not listed in SHA256SUMS``); digest differs (`` `x` does not match``); a listed file missing from the archive (``SHA256SUMS lists `x`, which the archive lacks``) |
| `Io(std::io::Error)` | `io error: {0}` | reading the input, or `Package::write_to` creating files/directories (including `AlreadyExists`) |
