// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports)]

#[path = "unit/cli_tests.rs"]
mod cli_tests;
#[path = "unit/client_tests.rs"]
mod client_tests;
#[path = "unit/commands_tests.rs"]
mod commands_tests;
#[path = "unit/config_tests.rs"]
mod config_tests;

quench_test::main!();
