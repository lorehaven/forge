// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::missing_const_for_fn)]

#[path = "unit/book_tests.rs"]
mod book_tests;
#[path = "unit/cli_tests.rs"]
mod cli_tests;
#[path = "unit/config_tests.rs"]
mod config_tests;
#[path = "unit/doctor_tests.rs"]
mod doctor_tests;
#[path = "unit/font_tests.rs"]
mod font_tests;
#[path = "unit/graphics_tests.rs"]
mod graphics_tests;
#[path = "unit/image_tests.rs"]
mod image_tests;
#[path = "unit/pager_tests.rs"]
mod pager_tests;
#[path = "unit/plain_tests.rs"]
mod plain_tests;
#[path = "unit/released_tests.rs"]
mod released_tests;
#[path = "unit/support.rs"]
mod support;
#[cfg(unix)]
#[path = "unit/terminal_tests.rs"]
mod terminal_tests;
#[path = "unit/view_tests.rs"]
mod view_tests;

quench_test::main!();
