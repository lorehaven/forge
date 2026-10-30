//! Sage in the browser: the screens of `sage-app` over `fetch` and `EventSource`, with the session cookie the service's sign-in
//! set.

pub mod browser;
pub mod client;

pub use browser::app;

/// The stylesheet of the page.
#[must_use]
pub fn stylesheet() -> String {
    sage_app::STYLE.to_string()
}
