//! Mailroom's wire model and forms. No I/O; builds for wasm.

#![allow(
    clippy::doc_markdown,
    clippy::too_long_first_doc_paragraph,
    clippy::struct_excessive_bools,
    clippy::missing_const_for_fn
)]

pub mod admin;
pub mod draft;
pub mod model;
pub mod time;

pub use draft::{ComposeDraft, DraftError, local_part_ok, recipient_name};
pub use model::*;
