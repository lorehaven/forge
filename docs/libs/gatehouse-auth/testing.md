---
title: Testing
parent: gatehouse-auth
grand_parent: Libraries
nav_order: 4
---

`tests/unit/lib_tests.rs` (wired by `tests/unit.rs`): file naming; `load` is `None` without a file; save and load round trip, found from a subdirectory; file mode is private; `.gitignore` gets the pattern once and is left alone when a wildcard covers it; a malformed file is reported; `bearer`; `access_token` against an unreachable server errors; a tool with a directory keeps its secret inside it, a new login leaves one token behind and the in-directory secret wins; extra values survive a new login. No test contacts a real Gatehouse. Run: `cargo test -p gatehouse-auth`.
