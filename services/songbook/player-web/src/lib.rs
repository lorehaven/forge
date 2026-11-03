//! The player in the browser: the screens of `songbook-player` over `fetch`, with the session cookie the service's sign-in set.

pub mod browser;
pub use songbook_player::protocol;

pub use browser::app;

/// What only a browser has: thin dark scroll bars, like the other services' (the phone draws its own).
const WEB_ONLY: &str = "
:root{--pl-scroll:#3b3b44;--pl-scroll-hover:#5a5a66}
@media (prefers-color-scheme:light){:root{--pl-scroll:#c4c4cc;--pl-scroll-hover:#9d9da8}}
*{scrollbar-width:thin;scrollbar-color:var(--pl-scroll) transparent}
*::-webkit-scrollbar{width:.5rem;height:.5rem}
*::-webkit-scrollbar-track{background:transparent}
*::-webkit-scrollbar-thumb{background-color:var(--pl-scroll);border-radius:.5rem;border:0}
*::-webkit-scrollbar-thumb:hover{background-color:var(--pl-scroll-hover)}
*::-webkit-scrollbar-corner{background:transparent}
";

/// The stylesheet of the page.
#[must_use]
pub fn stylesheet() -> String {
    format!("{}{WEB_ONLY}", songbook_player::STYLE)
}
