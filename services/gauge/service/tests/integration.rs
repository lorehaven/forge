// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::suboptimal_flops,
    clippy::unwrap_used
)]
#![allow(clippy::wildcard_imports)]

#[path = "integration/history_tests.rs"]
mod history_tests;

quench_test::main!();
