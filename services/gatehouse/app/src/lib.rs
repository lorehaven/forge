//! Gatehouse's screens: sign-in and the pages that need no session, the services dashboard, the account and the user
//! administration. The app does no I/O: it asks its host for the data (`live::Request`) and is told what came (`live::Msg`),
//! so the same screens run in the browser and in tests.
//!
//! The writes are form posts to the service (`POST /ui/login`, `/ui/account/password`...): the browser sends them, the service
//! answers with a redirect (and, for a sign-in, the session cookies), and the page that comes back shows what the query says.
//! That keeps the single-sign-on routes exactly as every other service and app expects them.

pub mod live;
pub mod screens;

pub use live::{Gatehouse, LANGUAGES, Msg, Request, Setup, gatehouse};

/// The app's own style on top of the framework's.
pub const STYLE: &str = include_str!("style.css");
