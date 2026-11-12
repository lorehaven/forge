//! Genealogy logic with no interface: it builds for the browser and the server alike, and its
//! tests run natively and fast.

// Drawing geometry: counts of a few hundred people and pixel sizes, rounded for display.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
// Dates and sectors are written the way their arithmetic is; module summaries are long.
#![allow(clippy::many_single_char_names, clippy::too_long_first_doc_paragraph)]

pub mod date;
pub mod fan;
pub mod gedcom;
pub mod legacy;
pub mod model;
pub mod text;
pub mod tree;
pub mod wire;

pub use date::{Date, Qualifier};
pub use model::{Citation, Event, Fact, Family, Person, Sex};
pub use tree::{Slot, Tree};
