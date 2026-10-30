// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports)]

mod support;

#[path = "unit/book_tests.rs"]
mod book_tests;
#[path = "unit/library_tests.rs"]
mod library_tests;
#[path = "unit/site_tests.rs"]
mod site_tests;
#[path = "unit/source_tests.rs"]
mod source_tests;
#[path = "unit/ui_tests.rs"]
mod ui_tests;

quench_test::main!();
