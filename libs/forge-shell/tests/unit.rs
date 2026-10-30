#![allow(clippy::unwrap_used)]

use std::rc::Rc;

use forge_shell::{Bar, Brand, PALETTES, flag, topbar};
use quench_ui::i18n::I18n;
use quench_ui::render_to_string;
use quench_ui::router::{MemoryHistory, Router};

const EN: &str = "locale_label = Language\nui_dashboard_button = Services dashboard\nui_home_button = Home\nui_theme_toggle = Toggle theme\nui_theme_mode = Mode\nui_theme_light = Light\nui_theme_dark = Dark\nui_theme_palette = Colour\nui_palette_green = Green\nui_palette_teal = Teal\nui_palette_blue = Blue\nui_palette_purple = Purple\nui_palette_rose = Rose\nui_palette_orange = Orange\nui_user_menu = User menu\nui_profile_edit = Edit profile\nui_logout = Log out\n";

#[quench_test::test]
fn a_flag_is_the_regions_two_letters() {
    assert_eq!(flag("pl-PL"), "\u{1F1F5}\u{1F1F1}");
    assert_eq!(flag("en_us"), "\u{1F1FA}\u{1F1F8}");
    assert_eq!(flag("en"), "");
    assert_eq!(flag("zh-Hans"), "");
}

#[quench_test::test]
fn the_bar_has_everything_the_old_pages_had() {
    let t = Rc::new(I18n::new(&[("en-US", EN)], "en-US").unwrap());
    let bar = Bar {
        brand: Brand {
            name: "Gauge",
            icon: "eye",
        },
        image: None,
        spa: true,
        account: true,
        brand_in_bar: false,
        home: "/gauge/ui/home".into(),
        logout: "/gauge/ui/logout".into(),
        on_logout: None,
        dashboard: true,
        languages: &["en-US", "pl-PL"],
        on_language: Rc::new(|_| {}),
    };
    let (html, scope) = quench_ui::create_root(|| {
        Router::new(Rc::new(MemoryHistory::new("/gauge/ui/home"))).provide();
        render_to_string(topbar(&bar, &t))
    });
    drop(scope);
    for needle in [
        "data-gh-href=\"/ui/home\"",
        "id=\"home\"",
        "fui-ico-eye",
        "Gauge",
        "\u{1F1F5}\u{1F1F1} pl-PL",
        "Toggle theme",
        "data-fui-mode=\"dark\"",
        "data-gh-href=\"/ui/account\"",
        "data-gh-src=\"/ui/account/avatar\"",
        "href=\"/gauge/ui/logout\"",
        "Log out",
    ] {
        assert!(html.contains(needle), "{needle}: {html}");
    }
    for palette in PALETTES {
        assert!(
            html.contains(&format!("data-fui-palette=\"{palette}\"")),
            "{palette}"
        );
    }
}

#[quench_test::test]
fn the_style_and_script_are_there() {
    assert!(forge_shell::STYLE.contains("--fui-bg: #0d0e10"));
    assert!(forge_shell::STYLE.contains(".fui-ico-eye"));
    assert!(forge_shell::SCRIPT.contains("qtheme"));
    assert!(forge_shell::SCRIPT.contains("data-drop-value"));
}

#[quench_test::test]
fn the_script_has_the_scroll_and_upload_hooks() {
    let script = forge_shell::SCRIPT;
    // scrolling to the element a dot names
    assert!(script.contains("[data-scroll-to]"));
    assert!(script.contains("scrollIntoView"));
    // a chosen file goes to the service as multipart, and the result comes back through an input event
    for needle in [
        "data-upload-url",
        "data-upload-fields",
        "data-upload-result",
        "new FormData()",
        "form.append('file', file, file.name)",
        "credentials: 'same-origin'",
        "new Event('input'",
        "JSON.stringify({ name: file.name, status: status, body: body })",
    ] {
        assert!(script.contains(needle), "{needle}");
    }
    // the upload hook leaves any other change event (the language picker) to its own handler
    assert!(script.contains("if (!url || !picker.files) return;"));
    // the script is still one well-formed function
    assert_eq!(script.matches('{').count(), script.matches('}').count());
    assert_eq!(script.matches('(').count(), script.matches(')').count());
}

quench_test::main!();
