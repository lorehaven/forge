// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(
    clippy::wildcard_imports,
    clippy::unwrap_used,
    clippy::too_many_lines,
    clippy::needless_pass_by_value
)]

mod support;

#[path = "unit/convert_tests.rs"]
mod convert_tests;
#[path = "unit/deezer_tests.rs"]
mod deezer_tests;
#[path = "unit/index_tests.rs"]
mod index_tests;
#[path = "unit/jobs_tests.rs"]
mod jobs_tests;
#[path = "unit/routers_tests.rs"]
mod routers_tests;
#[path = "unit/sync_tests.rs"]
mod sync_tests;
#[path = "unit/youtube_tests.rs"]
mod youtube_tests;

quench_test::main!();
