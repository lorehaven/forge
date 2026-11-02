//! The brain of Songbook's Android app, as a program of its own: it signs in to Gatehouse, talks to the Songbook API, makes the
//! tree that Android Auto and the phone browse, and keeps what was played. The Kotlin shell starts it and asks it things in
//! lines of JSON (see [`proto`]).

pub mod api;
pub mod auth;
pub mod proto;
pub mod store;
pub mod tree;

pub use auth::{Auth, Failure};
pub use proto::Brain;
