//! Mailroom - the estate's own mail. Every Gatehouse user has a mailbox (`name@<domain>`) with an inbox, sent mail and a trash;
//! people write to each other, and forge services (Gatehouse first) write to people through `POST /api/v1/send`. Mail lives in
//! Postgres. Nothing leaves the cluster: Mailroom opens no connection to any mail server and takes no mail from outside.

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

pub mod admin;
pub mod app;
pub mod boot;
pub mod config;
pub mod delivery;
pub mod drafts;
pub mod error;
pub mod limits;
pub mod mailbox;
pub mod outgoing;
pub mod routers;
pub mod settings;
pub mod store;
pub mod worker;
