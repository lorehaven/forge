// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports)]

#[path = "unit/condition_tests.rs"]
mod condition_tests;
#[path = "unit/cron_tests.rs"]
mod cron_tests;
#[path = "unit/graph_tests.rs"]
mod graph_tests;
#[path = "unit/parser_tests.rs"]
mod parser_tests;
#[path = "unit/selection_tests.rs"]
mod selection_tests;
#[path = "unit/spec_tests.rs"]
mod spec_tests;
#[path = "unit/steps_mod_tests.rs"]
mod steps_mod_tests;
#[path = "unit/steps_tools_tests.rs"]
mod steps_tools_tests;

quench_test::main!();
