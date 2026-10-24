// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports)]

#[path = "unit/lib_tests.rs"]
mod lib_tests;

quench_test::main!();
