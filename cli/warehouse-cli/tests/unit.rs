// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports)]

#[path = "unit/admin_api_tests.rs"]
mod admin_api_tests;
#[path = "unit/crates_api_tests.rs"]
mod crates_api_tests;
#[path = "unit/docker_api_tests.rs"]
mod docker_api_tests;
#[path = "unit/files_api_tests.rs"]
mod files_api_tests;

quench_test::main!();
