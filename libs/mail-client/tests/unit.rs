// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(
    clippy::pedantic,
    clippy::nursery,
    clippy::wildcard_imports,
    clippy::unwrap_used,
    clippy::expect_used
)]

#[path = "unit/address_tests.rs"]
mod address_tests;
#[path = "unit/charset_tests.rs"]
mod charset_tests;
#[path = "unit/compose_tests.rs"]
mod compose_tests;
#[path = "unit/date_tests.rs"]
mod date_tests;
#[path = "unit/encoding_tests.rs"]
mod encoding_tests;
#[path = "unit/header_tests.rs"]
mod header_tests;
#[path = "unit/imap_parse_tests.rs"]
mod imap_parse_tests;
#[path = "unit/imap_server_tests.rs"]
mod imap_server_tests;
#[path = "unit/message_tests.rs"]
mod message_tests;
#[path = "unit/netguard_tests.rs"]
mod netguard_tests;
#[path = "unit/presets_tests.rs"]
mod presets_tests;
#[path = "unit/sanitize_tests.rs"]
mod sanitize_tests;
#[path = "unit/smtp_tests.rs"]
mod smtp_tests;

quench_test::main!();
