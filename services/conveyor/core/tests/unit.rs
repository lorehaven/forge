// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports, clippy::unwrap_used)]

#[path = "unit/draft_tests.rs"]
mod draft_tests;
#[path = "unit/format_tests.rs"]
mod format_tests;
#[path = "unit/graph_tests.rs"]
mod graph_tests;
#[path = "unit/model_tests.rs"]
mod model_tests;
#[path = "unit/scan_tests.rs"]
mod scan_tests;
#[path = "unit/support.rs"]
mod support;
#[path = "unit/tree_tests.rs"]
mod tree_tests;

quench_test::main!();
