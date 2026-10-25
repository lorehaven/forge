//! The individual attack families. Each takes the shared [`Ctx`](crate::scan::Ctx) and one target.

pub mod auth;
pub mod cors;
pub mod headers;
pub mod injection;
pub mod protocol;
pub mod throttle;
pub mod tokens;
pub mod traversal;

use crate::finding::Severity;

/// Text that should never appear in a response to an outsider: stack traces, source paths,
/// database errors, secrets. Returns what matched and how bad it is.
#[must_use]
pub fn leak_marker(body: &str) -> Option<(Severity, &'static str)> {
    const MARKERS: [(&str, Severity, &str); 18] = [
        ("root:x:0:0", Severity::Critical, "contents of /etc/passwd"),
        ("root:*:0:0", Severity::Critical, "contents of /etc/passwd"),
        (
            "BEGIN PRIVATE KEY",
            Severity::Critical,
            "private key material",
        ),
        (
            "BEGIN RSA PRIVATE KEY",
            Severity::Critical,
            "private key material",
        ),
        (
            "BEGIN OPENSSH PRIVATE KEY",
            Severity::Critical,
            "private key material",
        ),
        ("[fonts]", Severity::Critical, "contents of win.ini"),
        ("panicked at", Severity::Medium, "a Rust panic message"),
        ("RUST_BACKTRACE", Severity::Medium, "a Rust backtrace hint"),
        ("stack backtrace:", Severity::Medium, "a stack backtrace"),
        ("thread '", Severity::Medium, "a thread panic report"),
        ("/home/", Severity::Medium, "a filesystem path"),
        ("/usr/src/", Severity::Medium, "a source path"),
        ("src/main.rs", Severity::Medium, "a source path"),
        (
            "syntax error at or near",
            Severity::High,
            "a PostgreSQL error",
        ),
        ("relation \"", Severity::High, "a PostgreSQL error"),
        ("SQLSTATE", Severity::High, "a database error code"),
        ("sqlx::", Severity::Medium, "a database library error"),
        (
            "password authentication failed",
            Severity::High,
            "a database credential error",
        ),
    ];
    MARKERS
        .iter()
        .find(|(needle, _, _)| body.contains(needle))
        .map(|(_, severity, what)| (*severity, *what))
}
