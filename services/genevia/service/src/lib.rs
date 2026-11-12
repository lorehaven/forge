//! Genevia - family trees. GEDCOM files are imported into Postgres (schema `genevia`, installed by Foundry), read back as
//! trees, and served as a JSON API with the browser client that draws them.

// Router `Result<T, Response>` early-returns trip this; boxing buys nothing.
#![allow(clippy::result_large_err)]
// The store holds a pooled connection for the length of a transaction; counts are small.
#![allow(
    clippy::significant_drop_tightening,
    clippy::cast_possible_truncation,
    clippy::too_many_lines
)]
// Module summaries are long.
#![allow(clippy::too_long_first_doc_paragraph)]

pub mod import;
pub mod routers;
pub mod store;
