//! Sage's screens: the chat with its sidebar, and the screen that waits for the models. The app does no I/O: it asks its host
//! for the data and for every change (`live::Request`) and is told what came (`live::Msg`), the pieces of a streaming answer
//! included, so the same screens run in the browser and in tests.

pub mod live;
pub mod screens;

pub use live::{LANGUAGES, Msg, Request, Sage, Setup, sage};

/// The app's own style on top of the framework's.
pub const STYLE: &str = include_str!("style.css");
