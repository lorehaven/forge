//! Songbook's admin screens: the library of followed artists, the pipeline that fills it, suggestions for new artists and the
//! log of the running job. The app does no I/O: it asks its host for what it shows and for every change (`live::Request`) and
//! is told what came (`live::Msg`), so the same screens run in the browser and in tests.

pub mod live;
pub mod screens;

pub use live::{LANGUAGES, Msg, Request, Setup, Songbook, songbook};

/// The app's own style on top of the framework's.
pub const STYLE: &str = include_str!("style.css");
