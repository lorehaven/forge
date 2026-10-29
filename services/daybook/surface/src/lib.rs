//! Daybook on Android: the device's side of the shared controller, done by the app's plugins through the surface bridge.

pub mod android;
pub mod launch;
pub mod widget;

pub use android::{Android, reminder_args};
