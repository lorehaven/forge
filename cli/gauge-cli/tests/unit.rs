#![allow(clippy::wildcard_imports, clippy::assert_is_empty)]

#[path = "unit/config_tests.rs"]
mod config_tests;
#[path = "unit/source_tests.rs"]
mod source_tests;
#[path = "unit/state_tests.rs"]
mod state_tests;
#[path = "unit/tiles_tests.rs"]
mod tiles_tests;
#[path = "unit/view_tests.rs"]
mod view_tests;

quench_test::main!();
