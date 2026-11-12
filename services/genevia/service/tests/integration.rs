// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(
    clippy::wildcard_imports,
    clippy::too_many_lines,
    clippy::many_single_char_names,
    clippy::future_not_send,
    clippy::unwrap_used
)]

#[path = "integration/api_tests.rs"]
mod api_tests;
#[path = "integration/support.rs"]
mod support;

quench_test::main!();
