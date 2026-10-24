// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports)]

#[path = "unit/authz_request_tests.rs"]
mod authz_request_tests;
#[path = "unit/authz_tests.rs"]
mod authz_tests;

quench_test::main!();
