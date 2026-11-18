//! Mailroom - the estate's mail. A web mail client over any IMAP/SMTP provider (the person adds a mailbox, Mailroom keeps a
//! cache of its headers and fetches bodies on demand), the one place forge services send mail from (a queue with retries,
//! sender identities, budgets), and the administrator's panel. It opens connections out to mail servers and takes none in.

// Router `Result<T, Response>` early-returns trip this; boxing buys nothing.
#![allow(clippy::result_large_err)]
#![allow(
    clippy::cast_possible_truncation,
    clippy::semicolon_if_nothing_returned,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::too_many_lines,
    clippy::significant_drop_tightening,
    clippy::similar_names,
    clippy::doc_markdown,
    clippy::too_long_first_doc_paragraph,
    clippy::struct_excessive_bools,
    clippy::missing_const_for_fn,
    clippy::option_if_let_else
)]

pub mod accounts;
pub mod admin;
pub mod app;
pub mod boot;
pub mod config;
pub mod connect;
pub mod error;
pub mod limits;
pub mod mailbox;
pub mod outgoing;
pub mod routers;
pub mod settings;
pub mod store;
pub mod sync;
pub mod vault;
pub mod worker;
