//! Songbook in the browser: the screens of `songbook-app` over `fetch`, with the session cookie the service's sign-in set.

pub mod browser;
pub mod protocol;

pub use browser::app;

/// The stylesheet of the page.
#[must_use]
pub fn stylesheet() -> String {
    songbook_app::STYLE.to_string()
}
