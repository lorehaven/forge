// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports)]

#[path = "unit/archive_tests.rs"]
mod archive_tests;
#[path = "unit/manifest_tests.rs"]
mod manifest_tests;
#[path = "unit/path_tests.rs"]
mod path_tests;

quench_test::main!();
