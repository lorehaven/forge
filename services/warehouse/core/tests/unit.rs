// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports, clippy::unwrap_used)]

#[path = "unit/draft_tests.rs"]
mod draft_tests;
#[path = "unit/fmt_tests.rs"]
mod fmt_tests;
#[path = "unit/model_tests.rs"]
mod model_tests;
#[path = "unit/tree_tests.rs"]
mod tree_tests;

quench_test::main!();
