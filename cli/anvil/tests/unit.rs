// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports, clippy::literal_string_with_formatting_args)]

#[path = "support.rs"]
mod support;

#[path = "unit/artifact_tests.rs"]
mod artifact_tests;
#[path = "unit/build_tests.rs"]
mod build_tests;
#[path = "unit/cargo_meta_tests.rs"]
mod cargo_meta_tests;
#[path = "unit/cli_tests.rs"]
mod cli_tests;
#[path = "unit/config_check_tests.rs"]
mod config_check_tests;
#[path = "unit/config_tests.rs"]
mod config_tests;
#[path = "unit/docker_tests.rs"]
mod docker_tests;
#[path = "unit/install_tests.rs"]
mod install_tests;
#[path = "unit/lint_tests.rs"]
mod lint_tests;
#[path = "unit/login_tests.rs"]
mod login_tests;
#[path = "unit/release_tests.rs"]
mod release_tests;
#[path = "unit/run_tests.rs"]
mod run_tests;
#[path = "unit/util_tests.rs"]
mod util_tests;
#[path = "unit/workspace_tests.rs"]
mod workspace_tests;

quench_test::main!();
