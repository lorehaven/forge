// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(
    clippy::pedantic,
    clippy::nursery,
    clippy::wildcard_imports,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::too_many_lines,
    clippy::type_complexity,
    clippy::cloned_ref_to_slice_refs,
    clippy::unused_async
)]

mod support;

#[path = "unit/accounts_tests.rs"]
mod accounts_tests;
#[path = "unit/api_tests.rs"]
mod api_tests;
#[path = "unit/mailbox_tests.rs"]
mod mailbox_tests;
#[path = "unit/outgoing_tests.rs"]
mod outgoing_tests;
#[path = "unit/pure_tests.rs"]
mod pure_tests;
#[path = "unit/sync_tests.rs"]
mod sync_tests;

quench_test::main!();
