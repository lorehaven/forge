//! Sage - the estate's AI workspace service.

// Router `Result<T, Response>` early-returns trip this; boxing buys nothing.
#![allow(clippy::result_large_err)]

pub mod clients;
pub mod config;
pub mod domain;
pub mod files;
pub mod observability;
pub mod routers;
pub mod runtime;
pub mod startup;
pub mod tools;
