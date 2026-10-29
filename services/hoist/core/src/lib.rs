//! Hoist's logic, with no interface and no I/O: it builds for the browser and the hosts alike, and its tests
//! run natively and fast.
//!
//! The whole app is a join of two lists: what the warehouse registry offers (`GET /api/v1/artifacts`) and what
//! is on this machine. Everything the screens show is a projection of that join, so it is pure functions over
//! plain data, tested before anything draws it.

pub mod model;
pub mod public;
pub mod updates;

pub use model::{Artifact, Installed, SELF_NAME, SELF_PROGRAM, format_bytes, version_code};
pub use public::{Category, PublicApp, PublicPage, PublicQuery, home_categories, suggestions};
pub use updates::{
    Entry, InstallState, Update, announcement_key, build_entries, pending_updates, state_for,
};

/// The counter after a press of a button (the scaffold's demo, until the catalog screen replaces it).
#[must_use]
pub const fn step(count: i64, delta: i64) -> i64 {
    count.saturating_add(delta)
}
