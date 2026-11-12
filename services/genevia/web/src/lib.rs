//! Genevia in the browser: the screens of `genevia-app` over `fetch`, with the session cookie the service's sign-in set.

pub mod browser;

pub use browser::app;

/// The stylesheet of the page.
#[must_use]
pub fn stylesheet() -> String {
    genevia_app::STYLE.to_string()
}
