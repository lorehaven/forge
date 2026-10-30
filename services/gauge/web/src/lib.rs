//! Gauge in the browser: the screens of `gauge-app` over `fetch`, with the session cookie the service's sign-in set.

pub mod browser;
pub mod protocol;

pub use browser::app;

/// The stylesheet of the page.
#[must_use]
pub fn stylesheet() -> String {
    gauge_app::STYLE.to_string()
}
