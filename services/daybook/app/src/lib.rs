//! The interface: sign-in, the lists, the tasks and their form. The app does no I/O: it asks its host for things
//! (`daybook_core::Request`) and is told what happened (`daybook_core::Msg`), so the same screens run on every host
//! and in tests.

pub mod form;
pub mod icons;
pub mod live;
pub mod pickers;
pub mod toast;

pub use live::{Daybook, Setup, daybook};

/// The app's own style on top of the framework's.
pub const STYLE: &str = include_str!("style.css");
