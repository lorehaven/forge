// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports)]

#[path = "unit/authz_tests.rs"]
mod authz_tests;
#[path = "unit/client_tests.rs"]
mod client_tests;
#[path = "unit/upload_tests.rs"]
mod upload_tests;

quench_test::main!();
