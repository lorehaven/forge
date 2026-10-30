//! Switchboard's screens: the home cards, the models registry and the vLLM instances with their launch and stop forms. The
//! app does no I/O: it asks its host for the data and for every change (`live::Request`) and is told what came
//! (`live::Msg`), so the same screens run in the browser and in tests.

pub mod live;
pub mod screens;

pub use live::{LANGUAGES, Msg, Request, Setup, Switchboard, switchboard};

/// The app's own style on top of the framework's.
pub const STYLE: &str = include_str!("style.css");
