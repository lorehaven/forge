---
title: Internals
parent: rivet-package
grand_parent: Libraries
nav_order: 5
---

## Read algorithm

`Package::read(reader, limits)`:

1. Read the reader to the end into memory.
2. zstd-decompress with a cap of `max_total_bytes + (max_entries + 2) * 1024` bytes (saturating, clamped to `usize`). Cap exceeded is `Limit`; any other zstd failure is `Archive("not a zstd stream: ...")`.
3. For each tar entry: skip directories; `path::validate(path)`; reject anything that is not a regular file (`UnsafePath`, "only regular files are allowed (no links or devices)"); reject when `max_entries` files are already held (`Limit`); reject `size > max_file_bytes`; add to the running total and reject `> max_total_bytes`; reject a repeated path (`Archive`).
4. Require `rivet.toml`, `overlay.yaml`, `SHA256SUMS` (`Missing`).
5. Parse `rivet.toml` (must be UTF-8) with `Manifest::parse`.
6. Parse `SHA256SUMS` and compare every file's SHA-256 to the listed digest; unlisted file, mismatch and phantom listing are `Checksum` errors.

Nothing is extracted through a filesystem API while reading. The only function that writes to a directory is `Package::write_to`.

## Properties

* **Deterministic**: sorted entries, fixed header metadata, no timestamps. The zstd output is stable for a given library version only; a registry should store the digest of the bytes it holds.
* **Bomb-resistant**: sizes are checked against declared entry sizes and, independently, the decoded stream is capped.
* **Memory**: the compressed input, the decoded tar and every file are in memory at once. With default limits that is bounded by roughly 128 MiB plus overhead for the tar, plus the file copies.
* **No links**: symlinks, hard links, devices and FIFOs are refused.
* **Not validated here**: that `name`/`version` in the manifest match a URL or file name (the warehouse service checks this), that `overlay.yaml` is valid YAML, that deployment resources exist in the overlay.
* **Thread-safety**: all types own their data; nothing is shared or interior-mutable.
