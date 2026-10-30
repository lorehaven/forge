// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports, clippy::unwrap_used)]

#[path = "unit/draft_tests.rs"]
mod draft_tests;
#[path = "unit/filter_tests.rs"]
mod filter_tests;
#[path = "unit/model_tests.rs"]
mod model_tests;

quench_test::main!();
