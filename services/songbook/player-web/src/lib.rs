//! The player in the browser: the screens of `songbook-player` over `fetch`, with the session cookie the service's sign-in set.

pub mod browser;
pub use songbook_player::protocol;

pub use browser::app;

/// The stylesheet of the page.
#[must_use]
pub fn stylesheet() -> String {
    songbook_player::STYLE.to_string()
}
