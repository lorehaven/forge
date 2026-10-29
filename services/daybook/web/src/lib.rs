//! Daybook in the browser: the screens of `daybook-app` and the controller of `daybook-host`, with `fetch` as the way to the
//! services and the page's storage for the session.

pub mod browser;
pub mod client;
pub mod platform;

pub use browser::app;

/// The stylesheet of the page: the framework's themes and components, then the app's own.
#[must_use]
pub fn stylesheet() -> String {
    use quench_style::theme::Theme;
    let mut css = String::new();
    for (scheme, theme) in [("dark", Theme::DefaultDark), ("light", Theme::DefaultLight)] {
        css.push_str(&format!(
            "@media (prefers-color-scheme: {scheme}) {{\n{}\n}}\n",
            Theme::theme(theme)
        ));
    }
    for sheet in [
        quench_style::tokens::stylesheet(),
        quench_style::mobile::stylesheet(),
        quench_style::components::stylesheet(),
    ] {
        css.push_str(&sheet);
        css.push('\n');
    }
    css.push_str(daybook_app::STYLE);
    css
}
pub use platform::Web;
