//! Showing a message's HTML safely: Mailroom's mail is written by other people and by services, so what reaches a
//! browser passes an allow-list first (no scripts, no remote content unless asked, no style that can load anything).

#![allow(
    clippy::cast_possible_truncation,
    clippy::semicolon_if_nothing_returned,
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

pub mod sanitize;
