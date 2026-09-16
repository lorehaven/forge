//! Workbench - the estate's task management service.

// Router `Result<T, Response>` early-returns trip this; boxing buys nothing.
#![allow(clippy::result_large_err)]

pub mod domain;
pub mod routers;
