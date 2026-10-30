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
    // The theme follows the person's choice in forge's theme menu (`html[data-fui-theme]`, set by the shell script), dark
    // until they pick light.
    let dark = Theme::theme(Theme::DefaultDark);
    css.push_str(&dark);
    css.push('\n');
    css.push_str(&dark.replace(":root", "html[data-fui-theme=\"dark\"]"));
    css.push('\n');
    css.push_str(
        &Theme::theme(Theme::DefaultLight).replace(":root", "html[data-fui-theme=\"light\"]"),
    );
    css.push('\n');
    for sheet in [
        quench_style::tokens::stylesheet(),
        quench_style::mobile::stylesheet(),
        quench_style::components::stylesheet(),
    ] {
        css.push_str(&sheet);
        css.push('\n');
    }
    css.push_str(daybook_app::STYLE);
    css.push('\n');
    css.push_str(FRAME);
    css
}
pub use platform::Web;

/// What puts the app under forge's top bar and in its colours.
const FRAME: &str = include_str!("../frame.css");
