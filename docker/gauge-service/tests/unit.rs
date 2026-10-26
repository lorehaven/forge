// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports)]

#[path = "unit/model_tests.rs"]
mod model_tests;
#[path = "unit/prom_tests.rs"]
mod prom_tests;
#[path = "unit/sources_tests.rs"]
mod sources_tests;

quench_test::main!();
