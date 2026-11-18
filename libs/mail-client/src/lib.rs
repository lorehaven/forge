//! Mail client protocols for Mailroom: reading a message (MIME), showing it safely (HTML sanitising), talking to a mailbox
//! (IMAP), writing one (composing), and deciding where Mailroom may connect (address policy). Sending goes through
//! `quench-mail`'s SMTP client; this crate wraps what it lacks (attachments) and the guard in front of it.

#![allow(
    clippy::cast_possible_truncation,
    clippy::semicolon_if_nothing_returned,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::too_many_lines,
    clippy::doc_markdown,
    clippy::too_long_first_doc_paragraph,
    clippy::missing_const_for_fn,
    clippy::option_if_let_else,
    clippy::similar_names,
    clippy::many_single_char_names,
    clippy::significant_drop_tightening,
    clippy::struct_excessive_bools
)]

pub mod address;
pub mod charset;
mod charset_tables;
pub mod compose;
pub mod date;
pub mod encoding;
pub mod entity;
pub mod header;
pub mod imap;
pub mod message;
pub mod netguard;
pub mod presets;
pub mod sanitize;
pub mod smtp;
