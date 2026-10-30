---
title: Testing
parent: rivet-package
grand_parent: Libraries
nav_order: 6
---

`tests/unit.rs` wires three modules under `tests/unit/` with the `quench-test` harness (`harness = false`):

* `archive_tests.rs`: round trip, byte-identical rebuild, insertion-order independence, builder rejections (reserved names, `..`, duplicates, missing overlay, invalid manifest), non-zstd input, traversal and absolute paths, link and device entries (crafted with `quench_archive::tar::Header` because a normal writer will not produce them), missing required files, checksum mismatch, files not listed, phantom listed files, duplicate entries, each limit, and a highly compressible archive stopped by the decoded-size cap.
* `manifest_tests.rs`: minimal manifest, build metadata, bad names and versions, unknown `[package]` keys, requirement syntax, `parse_package_requirement`, deployments.
* `path_tests.rs`: `path::validate` cases.

Run: `cargo test -p rivet-package` from `forge/`.

To test code that consumes packages, build one with `PackageBuilder` (no files on disk needed) and feed `to_bytes()` to `Package::read`.
