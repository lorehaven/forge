// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports)]

#[path = "unit/commands_tests.rs"]
mod commands_tests;
#[path = "unit/config_tests.rs"]
mod config_tests;
#[path = "unit/docker_tests.rs"]
mod docker_tests;
#[path = "unit/estate_tests.rs"]
mod estate_tests;
#[path = "unit/process_tests.rs"]
mod process_tests;
#[path = "unit/repl_tests.rs"]
mod repl_tests;
#[path = "unit/services_tests.rs"]
mod services_tests;
#[path = "unit/tasks_tests.rs"]
mod tasks_tests;
#[path = "unit/ui_tests.rs"]
mod ui_tests;
#[path = "unit/vars_tests.rs"]
mod vars_tests;

quench_test::main!();
