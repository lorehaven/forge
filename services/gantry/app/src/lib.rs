//! Gantry's screens: the packages, one package's resources and deployments, its versions, the operations and plans and the
//! YAML editor. The app does no I/O: it asks its host for the data and for every change (`live::Request`) and is told what
//! came (`live::Msg`), so the same screens run in the browser and in tests.

pub mod live;
pub mod screens;

pub use live::{Gantry, LANGUAGES, Msg, Request, Setup, gantry};

/// The app's own style on top of the framework's.
pub const STYLE: &str = include_str!("style.css");
