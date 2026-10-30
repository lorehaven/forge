//! Gauge's screens: the hosts, and one host with everything Gauge knows of it. The app does no I/O: it asks its host for
//! the numbers (`live::Request`) and is told what came (`live::Msg`), so the same screens run in the browser and in tests.

pub mod live;
pub mod screens;

pub use live::{Gauge, LANGUAGES, Msg, Request, Setup, gauge};

/// The app's own style on top of the framework's.
pub const STYLE: &str = include_str!("style.css");
