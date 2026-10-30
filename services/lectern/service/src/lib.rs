//! Lectern: the documentation books `anvil docs release` puts in a Warehouse files storage, served to anyone.
//!
//! Nothing here signs anybody in. The service identifies itself to Warehouse with its own client credentials,
//! keeps the newest release of every book in memory, and answers plain `GET`s.

pub mod book;
pub mod config;
pub mod library;
pub mod site;
pub mod source;
pub mod ui;
