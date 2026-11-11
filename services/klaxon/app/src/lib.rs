//! The interface: the list of alarms and the editor. The app does no I/O: it asks its host for things
//! (`live::Request`) and is told what happened (`live::Msg`), so the same screens run on every host and in tests.

pub mod icons;
pub mod live;
pub mod screens;

/// The app's own style on top of the framework's.
pub const STYLE: &str = include_str!("style.css");
