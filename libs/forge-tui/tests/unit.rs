// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(
    clippy::unwrap_used,
    clippy::missing_panics_doc,
    clippy::unreadable_literal
)]

#[path = "unit/color_tests.rs"]
mod color_tests;
#[path = "unit/console_tests.rs"]
mod console_tests;
#[path = "unit/line_tests.rs"]
mod line_tests;
#[path = "unit/theme_tests.rs"]
mod theme_tests;
#[path = "unit/widget_tests.rs"]
mod widget_tests;

quench_test::main!();
