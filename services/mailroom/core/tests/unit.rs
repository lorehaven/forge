// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(
    clippy::pedantic,
    clippy::nursery,
    clippy::wildcard_imports,
    clippy::unwrap_used,
    clippy::type_complexity
)]

#[path = "unit/draft_tests.rs"]
mod draft_tests;
#[path = "unit/model_tests.rs"]
mod model_tests;
#[path = "unit/time_tests.rs"]
mod time_tests;

quench_test::main!();
