// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(
    clippy::pedantic,
    clippy::nursery,
    clippy::wildcard_imports,
    clippy::unwrap_used,
    clippy::expect_used
)]

#[path = "unit/sanitize_tests.rs"]
mod sanitize_tests;

quench_test::main!();
