// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::unwrap_used, clippy::missing_panics_doc)]

#[path = "unit/support.rs"]
mod support;

#[path = "unit/page_tests.rs"]
mod page_tests;

#[path = "unit/nav_tests.rs"]
mod nav_tests;

#[path = "unit/link_tests.rs"]
mod link_tests;

#[path = "unit/book_tests.rs"]
mod book_tests;

quench_test::main!();
