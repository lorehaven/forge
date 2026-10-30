---
title: Testing
parent: conveyor-pipeline
grand_parent: Libraries
nav_order: 5
---

`tests/unit.rs` wires `tests/unit/*.rs` (`parser_tests`, `graph_tests`, `condition_tests`, `spec_tests`, `cron_tests`, `selection_tests`, `steps_mod_tests`, `steps_tools_tests`) under the `quench-test` harness (`harness = false`). They cover globbing and trigger matching, parse errors for every `SpecError`, cycle detection and plan propagation, condition parsing and evaluation, `only` selection, cron fields, macros, zones and DST gaps, and step validation for each tool.

Run: `cargo test -p conveyor-pipeline` from `forge/`. Because the crate does no I/O, tests need no services or fixtures; build pipelines as string literals and call `parse`.
