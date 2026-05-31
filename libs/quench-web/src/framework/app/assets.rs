use super::i18n::available_locales;
use crate::Theme;
use crate::framework::app::i18n::generate_translations_js;
use crate::framework::theme_shared;
use strum::IntoEnumIterator;

pub fn create_asset_files(default_theme: Theme, resources_prefix: &str) {
    let supported_themes = Theme::iter().collect::<Vec<_>>();
    let supported_locales = available_locales().unwrap_or_default();
    create_asset_files_with_options(
        default_theme,
        &supported_themes,
        &supported_locales,
        resources_prefix,
    );
}

pub fn create_asset_files_with_options(
    _default_theme: Theme,
    supported_themes: &[Theme],
    supported_locales: &[String],
    _resources_prefix: &str,
) {
    if let Err(err) = std::fs::create_dir_all("dist/assets/css/themes") {
        eprintln!("ERROR: failed to create css themes directory: {err}");
    }

    if let Err(err) = std::fs::create_dir_all("dist/assets/js") {
        eprintln!("ERROR: failed to create js directory: {err}");
    }

    if let Err(err) = std::fs::write("dist/assets/css/style.css", theme_shared()) {
        eprintln!("ERROR: failed to write style.css: {err}");
    }

    if let Err(err) = std::fs::write(
        "dist/assets/favicon.png",
        include_bytes!("../../../favicon.png"),
    ) {
        eprintln!("ERROR: failed to write favicon.png: {err}");
    }

    for theme in supported_themes {
        let theme_str = theme.to_string();
        let path = format!("dist/assets/css/themes/{theme_str}.css");

        if let Err(err) = std::fs::write(&path, Theme::theme(*theme)) {
            eprintln!("ERROR: failed to write theme file {path}: {err}");
        }
    }

    if let Err(err) = std::fs::write(
        "dist/assets/js/translations.js",
        generate_translations_js(supported_locales),
    ) {
        eprintln!("ERROR: failed to write translations.js: {err}");
    }
}
