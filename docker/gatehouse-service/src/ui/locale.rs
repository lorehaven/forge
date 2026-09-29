//! A user's saved `preferred_locale` as the browser's starting language.
//!
//! The UI language lives in the `qlocale` cookie, which every service's page
//! script fills in with the default on first load - so even the login page has
//! one before anyone signs in, and its presence says nothing. A choice made in
//! the language switch is marked separately (`qlocale_chosen`, see `forge-ui`).
//! The saved preference applies unless that marker is there: an explicit choice
//! always wins.

use crate::ui::common::SUPPORTED_LOCALES;
use async_trait::async_trait;
use quench_auth::http::domain::cookies::cookie_value;
use quench_http::prelude::{FromRequest, HttpError, Request};

pub const LOCALE_COOKIE: &str = "qlocale";
use forge_ui::LOCALE_CHOSEN_COOKIE;
const ONE_YEAR_SECS: u64 = 365 * 24 * 60 * 60;

fn is_supported(locale: &str) -> bool {
    SUPPORTED_LOCALES.contains(&locale)
}

/// Whether the browser holds an explicit language choice: the marker, plus a
/// `qlocale` this estate can still honour.
pub struct LocaleCookie(pub bool);

#[async_trait]
impl FromRequest for LocaleCookie {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
        let chosen = cookie_value(req, LOCALE_CHOSEN_COOKIE).is_some();
        let usable = cookie_value(req, LOCALE_COOKIE).is_some_and(|value| is_supported(&value));
        Ok(Self(chosen && usable))
    }
}

/// The `Set-Cookie` value to send at sign-in, if any: only when there is no explicit
/// choice and the saved preference is a locale this estate actually supports.
pub fn default_locale_cookie(has_cookie: &LocaleCookie, preferred: Option<&str>) -> Option<String> {
    let preferred = preferred.filter(|locale| is_supported(locale))?;
    if has_cookie.0 {
        return None;
    }
    // Not HttpOnly: the page script reads and rewrites it.
    Some(format!(
        "{LOCALE_COOKIE}={preferred}; Max-Age={ONE_YEAR_SECS}; Path=/; SameSite=Lax"
    ))
}
