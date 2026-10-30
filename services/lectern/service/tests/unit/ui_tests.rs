use lectern_service::ui::{Texts, cookie, mode, palette, pick_locale};

#[quench_test::test]
fn cookies_are_read_by_name() {
    let header = Some("a=1; qlocale=fr-FR;  fui_palette=blue");
    assert_eq!(cookie(header, "qlocale"), Some("fr-FR"));
    assert_eq!(cookie(header, "fui_palette"), Some("blue"));
    assert_eq!(cookie(header, "missing"), None);
    assert_eq!(cookie(None, "a"), None);
}

#[quench_test::test]
fn the_language_is_the_cookie_then_the_browser_then_english() {
    assert_eq!(pick_locale(Some("qlocale=es-ES"), Some("de")), "es-ES");
    assert_eq!(
        pick_locale(Some("qlocale=xx-XX"), Some("fr;q=0.8")),
        "fr-FR"
    );
    assert_eq!(pick_locale(None, Some("sv, pl-PL;q=0.9")), "pl-PL");
    assert_eq!(pick_locale(None, Some("sv")), "en-US");
    assert_eq!(pick_locale(None, None), "en-US");
}

#[quench_test::test]
fn mode_and_palette_follow_the_rules_of_the_shell_script() {
    assert_eq!(mode(Some("qtheme=default-light")), "light");
    assert_eq!(mode(Some("qtheme=default-dark")), "dark");
    assert_eq!(mode(None), "dark");
    assert_eq!(palette(Some("fui_palette=orange")), "orange");
    assert_eq!(palette(Some("fui_palette=chartreuse")), "green");
    assert_eq!(palette(None), "green");
}

#[quench_test::test]
fn every_language_has_every_word_the_page_uses() {
    let english = Texts::new("en-US");
    for locale in ["de-DE", "es-ES", "fr-FR", "pl-PL"] {
        let t = Texts::new(locale);
        for key in [
            "locale_label",
            "ui_home_button",
            "ui_theme_toggle",
            "ui_theme_mode",
            "ui_theme_light",
            "ui_theme_dark",
            "ui_theme_palette",
            "ui_palette_green",
            "ui_palette_orange",
            "ui_title",
            "ui_subtitle",
            "ui_book_meta",
            "ui_no_books",
        ] {
            assert_ne!(t.t(key), key, "{locale} lacks {key}");
        }
        assert_ne!(t.t("ui_title"), "", "{locale}");
        let _ = &english;
    }
}

#[quench_test::test]
fn placeholders_are_filled_and_an_unknown_key_is_itself() {
    let t = Texts::new("en-US");
    assert_eq!(
        t.format("ui_book_meta", &[("version", "9"), ("files", "3")]),
        "Release 9 · 3 pages"
    );
    assert_eq!(t.t("no_such_key"), "no_such_key");
    assert_eq!(Texts::new("xx-XX").locale(), "en-US");
}
