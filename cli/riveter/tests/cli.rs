//! End-to-end tests of the `riveter` binary: it runs as a child process in a temporary project, with
//! recording stand-ins for the tools it shells out to. Kept apart from `unit`, which holds what runs
//! in-process.

// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports)]

#[path = "cli/binary_tests.rs"]
mod binary_tests;

quench_test::main!();
