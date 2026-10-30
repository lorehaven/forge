// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports)]

#[path = "unit/alert_log_tests.rs"]
mod alert_log_tests;
#[path = "unit/history_tests.rs"]
mod history_tests;
#[path = "unit/prom_tests.rs"]
mod prom_tests;
#[path = "unit/sources_tests.rs"]
mod sources_tests;
#[path = "unit/store_tests.rs"]
mod store_tests;

quench_test::main!();
