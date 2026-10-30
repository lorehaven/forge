// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports, clippy::unwrap_used)]

#[path = "unit/api_tests.rs"]
mod api_tests;
#[path = "unit/chart_tests.rs"]
mod chart_tests;
#[path = "unit/forecast_tests.rs"]
mod forecast_tests;
#[path = "unit/format_tests.rs"]
mod format_tests;
#[path = "unit/model_tests.rs"]
mod model_tests;

quench_test::main!();
