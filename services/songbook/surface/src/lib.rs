//! Songbook on Android, drawn by the native renderer: the screens of `songbook-player` over the service (through the same
//! sign-in the playback service uses), pictures fetched with the person's token, and the sound made by the host's `player`
//! plugin.

pub mod app;
pub mod pictures;
pub mod pool;
pub mod signin;

/// The style of what the app adds to the player's screens (the sign-in).
pub const STYLE: &str = include_str!("style.css");

/// Gives the renderer every stylesheet of the app: the kit's (light and dark by the system's scheme), the player's, the app's.
pub fn add_styles(renderer: &quench_ui_native::NativeRenderer) {
    use quench_style::theme::Theme;
    for (scheme, theme) in [("dark", Theme::DefaultDark), ("light", Theme::DefaultLight)] {
        let rules = Theme::theme(theme);
        renderer.add_stylesheet(
            &[
                "@media (prefers-color-scheme: ",
                scheme,
                ") {\n",
                &rules,
                "\n}",
            ]
            .concat(),
        );
    }
    for sheet in [
        quench_style::tokens::stylesheet(),
        quench_style::mobile::stylesheet(),
        quench_style::components::stylesheet(),
    ] {
        renderer.add_stylesheet(&sheet);
    }
    renderer.add_stylesheet(songbook_player::STYLE);
    renderer.add_stylesheet(STYLE);
}
