// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(
    clippy::wildcard_imports,
    clippy::unwrap_used,
    clippy::too_many_lines,
    clippy::similar_names
)]

#[path = "unit/connectors_tests.rs"]
mod connectors_tests;
#[path = "unit/merge_tests.rs"]
mod merge_tests;

quench_test::main!();
