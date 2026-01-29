#![forbid(unsafe_code)]
#![deny(clippy::all)]
#![deny(clippy::cargo)]
#![deny(clippy::complexity)]
#![deny(clippy::correctness)]
#![deny(clippy::nursery)]
#![deny(clippy::pedantic)]
#![deny(clippy::perf)]
#![deny(clippy::style)]
#![deny(clippy::suspicious)]
#![deny(missing_debug_implementations)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::multiple_crate_versions)]
#![cfg_attr(test, deny(rust_2018_idioms))]

pub mod config;
pub mod core;
pub mod llm;
pub mod tools;
pub mod ui;
