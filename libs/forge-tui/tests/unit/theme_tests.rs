use forge_tui::{Mode, Palette, Rgb, Theme, Tone};
use std::collections::HashMap;

/// `--name: #rrggbb` pairs of a CSS block (the first `{ ... }` after `selector`).
fn block_vars(css: &str, selector: &str) -> HashMap<String, Rgb> {
    let start = css
        .find(selector)
        .unwrap_or_else(|| panic!("no {selector}"));
    let body = &css[start..];
    let end = body.find("\n}").unwrap();
    let mut vars = HashMap::new();
    for decl in body[..end].split(';') {
        let Some(at) = decl.rfind("--") else { continue };
        let Some((name, value)) = decl[at..].split_once(':') else {
            continue;
        };
        let value = value.trim();
        if value.len() == 7 && value.starts_with('#') {
            let n = u32::from_str_radix(&value[1..], 16).unwrap();
            vars.insert(name.trim().to_string(), Rgb::hex(n));
        }
    }
    vars
}

fn tokens_css() -> Option<String> {
    let path = format!(
        "{}/../forge-shell/assets/tokens.css",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read_to_string(path).ok()
}

#[quench_test::test]
fn dark_tokens_match_the_stylesheet() {
    let Some(css) = tokens_css() else { return };
    let v = block_vars(&css, ":root, html[data-fui-theme=\"dark\"]");
    let t = Theme::new(Mode::Dark, Palette::Green);
    assert_eq!(v["--fui-bg"], t.bg);
    assert_eq!(v["--fui-bg-sunken"], t.bg_sunken);
    assert_eq!(v["--fui-surface"], t.surface);
    assert_eq!(v["--fui-surface-2"], t.surface_2);
    assert_eq!(v["--fui-surface-hover"], t.surface_hover);
    assert_eq!(v["--fui-border"], t.border);
    assert_eq!(v["--fui-border-strong"], t.border_strong);
    assert_eq!(v["--fui-text"], t.text);
    assert_eq!(v["--fui-text-secondary"], t.text_secondary);
    assert_eq!(v["--fui-text-tertiary"], t.text_tertiary);
    assert_eq!(v["--fui-text-disabled"], t.text_disabled);
    assert_eq!(v["--fui-error-soft-text"], t.soft(Tone::Error).text);
    assert_eq!(v["--fui-warning-soft-text"], t.soft(Tone::Warning).text);
    assert_eq!(v["--fui-success-soft-text"], t.soft(Tone::Success).text);
    assert_eq!(v["--fui-blue-soft-text"], t.soft(Tone::Info).text);
}

#[quench_test::test]
fn light_tokens_match_the_stylesheet() {
    let Some(css) = tokens_css() else { return };
    let gray = block_vars(&css, ":root {");
    let t = Theme::new(Mode::Light, Palette::Green);
    // The light block refers to the raw gray scale, which is declared in `:root`.
    assert_eq!(gray["--fui-gray-50"], t.bg);
    assert_eq!(gray["--fui-gray-100"], t.bg_sunken);
    assert_eq!(gray["--fui-gray-200"], t.border);
    assert_eq!(gray["--fui-gray-300"], t.border_strong);
    assert_eq!(gray["--fui-gray-900"], t.text);
    assert_eq!(gray["--fui-gray-700"], t.text_secondary);
    assert_eq!(gray["--fui-gray-500"], t.text_tertiary);
    assert_eq!(gray["--fui-gray-400"], t.text_disabled);
    assert_eq!(gray["--fui-error-700"], t.soft(Tone::Error).text);
    assert_eq!(gray["--fui-warning-50"], t.soft(Tone::Warning).bg);
    assert_eq!(gray["--fui-success-200"], t.soft(Tone::Success).border);
}

#[quench_test::test]
fn every_palette_matches_the_stylesheet() {
    let Some(css) = tokens_css() else { return };
    for palette in Palette::ALL {
        let v = block_vars(
            &css,
            &format!("html[data-fui-palette=\"{}\"]", palette.name()),
        );
        let b = palette.brand();
        assert_eq!(v["--fui-brand-50"], b.c50, "{}", palette.name());
        assert_eq!(v["--fui-brand-200"], b.c200, "{}", palette.name());
        assert_eq!(v["--fui-brand-300"], b.c300, "{}", palette.name());
        assert_eq!(v["--fui-brand-500"], b.c500, "{}", palette.name());
        assert_eq!(v["--fui-brand-600"], b.c600, "{}", palette.name());
        assert_eq!(v["--fui-brand-700"], b.c700, "{}", palette.name());
    }
}

#[quench_test::test]
fn mode_and_palette_come_from_the_variables() {
    let t = Theme::from_vars("light", "", "purple");
    assert_eq!((t.mode, t.palette), (Mode::Light, Palette::Purple));
    // FORGE_THEME beats COLORFGBG; COLORFGBG decides when it is unset; dark is the default.
    assert_eq!(Theme::from_vars("dark", "0;15", "").mode, Mode::Dark);
    assert_eq!(Theme::from_vars("", "0;15", "").mode, Mode::Light);
    assert_eq!(Theme::from_vars("", "15;0", "").mode, Mode::Dark);
    assert_eq!(Theme::from_vars("", "", "nonsense"), Theme::dark());
}

#[quench_test::test]
fn dark_soft_tones_tint_the_surface() {
    let t = Theme::dark();
    let error = t.soft(Tone::Error);
    // Redder than the surface it sits on, still dark.
    assert!(
        error.bg.0 > t.surface.0 && error.bg.0 < 0x60,
        "{:?}",
        error.bg
    );
    assert_ne!(error.border, error.bg);
}
