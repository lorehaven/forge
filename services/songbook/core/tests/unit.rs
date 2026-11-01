#![allow(clippy::wildcard_imports, clippy::unwrap_used)]

#[path = "unit/api_tests.rs"]
mod api_tests;
#[path = "unit/naming_tests.rs"]
mod naming_tests;

quench_test::main!();
