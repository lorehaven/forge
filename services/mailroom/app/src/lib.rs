//! Mailroom's screens: the mail client (a unified inbox, folders, the reader, compose, mailboxes) and the administrator's panel.
//! The app does no I/O: it asks its host for the data and for every change (`live::Request`) and is told what came
//! (`live::Msg`), so the same screens run in the browser and in tests.

// Screens are one long view tree each; clones are of handles (signals, `Rc`s) moved into closures.
#![allow(
    clippy::too_many_lines,
    clippy::semicolon_if_nothing_returned,
    clippy::similar_names,
    clippy::redundant_clone,
    clippy::option_if_let_else,
    clippy::doc_markdown,
    clippy::too_long_first_doc_paragraph,
    clippy::map_unwrap_or,
    clippy::redundant_closure_for_method_calls,
    clippy::needless_pass_by_value,
    clippy::significant_drop_tightening
)]

pub mod live;
pub mod screens;

pub use live::{LANGUAGES, ListQuery, Mailroom, Msg, Request, Setup, mailroom};

/// The app's own style on top of the framework's.
pub const STYLE: &str = include_str!("style.css");
