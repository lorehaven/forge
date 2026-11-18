//! In-process IMAP and SMTP servers for tests: real sockets, real TLS (a fixture certificate for `localhost`), in-memory
//! mailboxes, and the controls a test needs (add mail, change what another client would, make the next send fail).
//! Used by `mail-client`'s tests, Mailroom's tests and the BDD suite.

#![allow(
    clippy::pedantic,
    clippy::nursery,
    clippy::chunks_exact_to_as_chunks,
    clippy::too_many_lines,
    clippy::missing_const_for_fn,
    clippy::doc_markdown,
    clippy::too_long_first_doc_paragraph,
    clippy::significant_drop_tightening,
    clippy::needless_pass_by_value,
    clippy::cast_possible_truncation,
    clippy::option_if_let_else,
    clippy::unused_async,
    clippy::similar_names,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::struct_excessive_bools
)]

mod imap_server;
mod io;
mod smtp_server;

pub use imap_server::{ImapOptions, ImapServer, StoredMessage};
pub use io::CERT_PEM;
pub use smtp_server::{Received, SmtpOptions, SmtpServer};
