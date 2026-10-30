//! The page Lectern draws itself: the list of books, in the frame the other forge services wear.
//!
//! The style, the script and the markup of the top bar are `forge-shell`'s, so the page follows the same light and dark
//! mode, palette and language cookies as every other service. What is left out is what Lectern does not have: there is
//! no account menu and no link to the services dashboard, because nobody signs in here.

use crate::book::Book;
use std::collections::HashMap;
use std::fmt::Write;
use std::sync::{Arc, LazyLock};

/// The languages of the page, the first is the fallback.
pub const LOCALES: [&str; 5] = ["en-US", "de-DE", "es-ES", "fr-FR", "pl-PL"];

const FALLBACK: &str = "en-US";

static CATALOGS: LazyLock<HashMap<&'static str, HashMap<String, String>>> = LazyLock::new(|| {
    [
        ("en-US", include_str!("../i18n/en-US.ftl")),
        ("de-DE", include_str!("../i18n/de-DE.ftl")),
        ("es-ES", include_str!("../i18n/es-ES.ftl")),
        ("fr-FR", include_str!("../i18n/fr-FR.ftl")),
        ("pl-PL", include_str!("../i18n/pl-PL.ftl")),
    ]
    .into_iter()
    .map(|(locale, text)| (locale, parse(text)))
    .collect()
});

/// The `key = value` lines of a translation file; comments and blank lines are skipped.
fn parse(text: &str) -> HashMap<String, String> {
    text.lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| (key.trim().to_string(), value.trim().to_string()))
        .collect()
}

/// The words of one language.
#[derive(Debug, Clone, Copy)]
pub struct Texts {
    locale: &'static str,
}

impl Texts {
    #[must_use]
    pub fn new(locale: &str) -> Self {
        Self {
            locale: LOCALES
                .iter()
                .copied()
                .find(|l| *l == locale)
                .unwrap_or(FALLBACK),
        }
    }

    #[must_use]
    pub const fn locale(&self) -> &'static str {
        self.locale
    }

    /// The text for `key`; the English one when this language lacks it, the key when both do.
    #[must_use]
    pub fn t(&self, key: &str) -> String {
        CATALOGS
            .get(self.locale)
            .and_then(|catalog| catalog.get(key))
            .or_else(|| CATALOGS.get(FALLBACK).and_then(|catalog| catalog.get(key)))
            .cloned()
            .unwrap_or_else(|| key.to_string())
    }

    /// [`t`](Self::t) with each `{ $name }` replaced.
    #[must_use]
    pub fn format(&self, key: &str, values: &[(&str, &str)]) -> String {
        let mut text = self.t(key);
        for (name, value) in values {
            text = text.replace(&format!("{{ ${name} }}"), value);
        }
        text
    }
}

/// The value of cookie `name` in a `Cookie` header.
#[must_use]
pub fn cookie<'a>(header: Option<&'a str>, name: &str) -> Option<&'a str> {
    header?.split(';').find_map(|pair| {
        let (key, value) = pair.trim().split_once('=')?;
        (key == name).then_some(value)
    })
}

/// The language to speak: the `qlocale` cookie when it names one we have, else the first language of
/// `Accept-Language` we have (`pl` finds `pl-PL`), else English.
#[must_use]
pub fn pick_locale(cookie_header: Option<&str>, accept_language: Option<&str>) -> &'static str {
    let have = |wanted: &str| {
        LOCALES.iter().copied().find(|locale| {
            locale.eq_ignore_ascii_case(wanted)
                || locale.split('-').next().is_some_and(|language| {
                    language.eq_ignore_ascii_case(wanted.split('-').next().unwrap_or_default())
                })
        })
    };
    if let Some(found) = cookie(cookie_header, "qlocale").and_then(have) {
        return found;
    }
    accept_language
        .into_iter()
        .flat_map(|list| list.split(','))
        .filter_map(|entry| entry.split(';').next())
        .map(str::trim)
        .find_map(have)
        .unwrap_or(FALLBACK)
}

/// `light` when the `qtheme` cookie says so, else `dark`: the rule `shell.js` follows.
#[must_use]
pub fn mode(cookie_header: Option<&str>) -> &'static str {
    if cookie(cookie_header, "qtheme").is_some_and(|theme| theme.contains("light")) {
        "light"
    } else {
        "dark"
    }
}

/// The palette named by the `fui_palette` cookie, `green` otherwise.
#[must_use]
pub fn palette(cookie_header: Option<&str>) -> &'static str {
    cookie(cookie_header, "fui_palette")
        .and_then(|name| forge_shell::PALETTES.iter().copied().find(|p| *p == name))
        .unwrap_or("green")
}

/// forge's stylesheet, then Lectern's.
#[must_use]
pub fn style() -> String {
    format!("{}\n{}", forge_shell::STYLE, include_str!("lectern.css"))
}

/// forge's script.
pub const SHELL_SCRIPT: &str = forge_shell::SCRIPT;

/// Lectern's own script: the page is drawn by the server, so a changed language is a reload.
pub const SCRIPT: &str = "document.addEventListener('change', function (e) {\n  \
if (e.target && e.target.id === 'language') setTimeout(function () { location.reload(); }, 50);\n});\n";

/// The script of a framed page: keeps the address bar and the title on the page the frame shows, and sends links that
/// leave the site to a tab of their own.
pub const FRAME_SCRIPT: &str = "(function () {\n  \
var frame = document.getElementById('book-frame');\n  \
if (!frame) return;\n  \
if (location.hash) frame.src = frame.getAttribute('src') + location.hash;\n  \
var last = '';\n  \
function sync() {\n    \
try {\n      \
var win = frame.contentWindow, here = win.location;\n      \
if (here.href === 'about:blank') return;\n      \
var path = here.pathname + here.search + here.hash;\n      \
if (path !== location.pathname + location.search + location.hash) history.replaceState(null, '', path);\n      \
var title = win.document.title;\n      \
if (title && title !== last) { last = title; document.title = title + ' \\u00b7 Lectern'; }\n      \
win.document.querySelectorAll('a[href]').forEach(function (a) {\n        \
try { if (new URL(a.href, here.href).origin !== here.origin) { a.target = '_blank'; a.rel = 'noopener noreferrer'; } } catch (e) { /* not a link */ }\n      \
});\n    \
} catch (e) { /* a page of another origin cannot be read */ }\n  \
}\n  \
frame.addEventListener('load', sync);\n  \
setInterval(sync, 1000);\n})();\n";

/// The paths of the page's files, below the base path.
pub const STYLE_PATH: &str = "/ui/assets/style.css";
pub const SHELL_PATH: &str = "/ui/assets/shell.js";
pub const SCRIPT_PATH: &str = "/ui/assets/lectern.js";
pub const ICON_PATH: &str = "/ui/assets/favicon.png";
pub const FRAME_PATH: &str = "/ui/assets/frame.js";

/// The names a book cannot have, because Lectern's own paths are there.
pub const RESERVED: [&str; 3] = ["api", "health", "ui"];

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn icon(name: &str) -> String {
    format!("<i class=\"fui-ico fui-ico-{name}\"></i>")
}

fn theme_menu(t: &Texts) -> String {
    let mode_button = |value: &str, glyph: &str, key: &str| {
        format!(
            "<button type=\"button\" class=\"fui-theme-mode\" data-mode=\"{value}\" data-fui-mode=\"{value}\">{}<span>{}</span></button>",
            icon(glyph),
            escape(&t.t(key))
        )
    };
    let mut swatches = String::new();
    for name in forge_shell::PALETTES {
        let label = escape(&t.t(&format!("ui_palette_{name}")));
        let _ = write!(
            swatches,
            "<button type=\"button\" class=\"fui-swatch\" data-palette=\"{name}\" data-fui-palette=\"{name}\" title=\"{label}\"><span class=\"sr-only\">{label}</span></button>"
        );
    }
    let toggle = escape(&t.t("ui_theme_toggle"));
    format!(
        "<details class=\"fui-dropdown\"><summary class=\"fui-icon-btn\" title=\"{toggle}\">{palette}<span class=\"sr-only\">{toggle}</span></summary>\
<div class=\"fui-dropdown-menu fui-theme-menu\">\
<div><div class=\"fui-theme-label\">{mode_label}</div><div class=\"fui-theme-modes\">{light}{dark}</div></div>\
<div><div class=\"fui-theme-label\">{palette_label}</div><div class=\"fui-theme-palettes\">{swatches}</div></div>\
</div></details>",
        palette = icon("palette"),
        mode_label = escape(&t.t("ui_theme_mode")),
        light = mode_button("light", "sun", "ui_theme_light"),
        dark = mode_button("dark", "moon", "ui_theme_dark"),
        palette_label = escape(&t.t("ui_theme_palette")),
    )
}

/// The books to jump between, and the one being read.
type Switcher<'a> = Option<(&'a [Arc<Book>], &'a str)>;

/// The brand, and after it where to go next: the book being read, with the other books in a menu.
fn switcher(base: &str, books: &[Arc<Book>], current: &str) -> String {
    let title = books
        .iter()
        .find(|book| book.name == current)
        .map_or(current, |book| book.title.as_str());
    let mut items = String::new();
    for book in books {
        let aria = if book.name == current {
            " aria-current=\"page\""
        } else {
            ""
        };
        let _ = write!(
            items,
            "<a class=\"fui-dropdown-item\" href=\"{base}/{name}/\" data-book=\"{name}\"{aria}>{file}<span>{title}</span></a>",
            base = escape(base),
            name = escape(&book.name),
            title = escape(&book.title),
            file = icon("file"),
        );
    }
    format!(
        "<details id=\"books-menu\" class=\"fui-dropdown lc-books\"><summary class=\"lc-books-summary\" title=\"{all}\">\
<span>{title}</span><i class=\"lc-chevron\"></i></summary><div class=\"fui-dropdown-menu lc-books-menu\">{items}</div></details>",
        all = escape(current),
        title = escape(title),
    )
}

fn topbar(base: &str, t: &Texts, books: Switcher<'_>) -> String {
    let mut options = String::new();
    for code in LOCALES {
        let selected = if code == t.locale() { " selected" } else { "" };
        let _ = write!(
            options,
            "<option value=\"{code}\"{selected}>{} {code}</option>",
            forge_shell::flag(code)
        );
    }
    let home = escape(&t.t("ui_home_button"));
    format!(
        "<header class=\"fui-topbar\">\
<div class=\"fui-topbar-nav\"><a id=\"home\" class=\"fui-icon-btn\" href=\"{base}/\" title=\"{home}\">{house}<span class=\"sr-only\">{home}</span></a></div>\
<a id=\"title\" class=\"fui-brand\" href=\"{base}/\"><span class=\"fui-brand-mark fui-brand-image\"><img src=\"{base}{ICON_PATH}\" alt=\"\"></span><span class=\"fui-brand-name\">Lectern</span></a>{switcher}\
<div class=\"fui-topbar-spacer\"></div>\
<div class=\"fui-topbar-actions\"><div class=\"locale-switch\"><select id=\"language\" aria-label=\"{language}\">{options}</select></div>{theme}</div>\
</header>",
        base = escape(base),
        house = icon("house"),
        language = escape(&t.t("locale_label")),
        theme = theme_menu(t),
        switcher = books.map_or_else(String::new, |(books, current)| switcher(
            base, books, current
        )),
    )
}

fn card(base: &str, t: &Texts, book: &Book) -> String {
    let meta = t.format(
        "ui_book_meta",
        &[
            ("version", &book.version),
            ("files", &book.file_count().to_string()),
        ],
    );
    format!(
        "<a class=\"fui-link-card lc-card\" href=\"{base}/{name}/\" data-book=\"{name}\">\
<span class=\"fui-link-card-icon\">{file}</span>\
<div class=\"fui-link-card-body\"><div class=\"fui-link-card-title\">{title}</div><div class=\"fui-link-card-desc\">{meta}</div></div>\
<div class=\"fui-link-card-arrow\">{arrow}</div></a>",
        base = escape(base),
        name = escape(&book.name),
        title = escape(&book.title),
        meta = escape(&meta),
        file = icon("file"),
        arrow = icon("arrow-right"),
    )
}

/// The list of books as a page.
///
/// `base` is the path Lectern is mounted at (`""` for the root); `cookies` and `accept_language` are the request's
/// headers, which decide the language, the mode and the palette.
#[must_use]
pub fn front_page(
    base: &str,
    books: &[Arc<Book>],
    cookies: Option<&str>,
    accept_language: Option<&str>,
) -> String {
    let t = Texts::new(pick_locale(cookies, accept_language));
    let cards = if books.is_empty() {
        format!(
            "<p id=\"no-books\" class=\"fui-empty\">{}</p>",
            escape(&t.t("ui_no_books"))
        )
    } else {
        let cards: String = books.iter().map(|book| card(base, &t, book)).collect();
        format!("<div id=\"books\" class=\"lc-grid\">{cards}</div>")
    };
    let title = escape(&t.t("ui_title"));
    format!(
        "<!doctype html>\n<html lang=\"{lang}\" data-fui-theme=\"{mode}\" data-fui-palette=\"{palette}\">\
<head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
<title>{title} · Lectern</title>\
<link rel=\"icon\" type=\"image/png\" href=\"{base}{ICON_PATH}\">\
<link rel=\"stylesheet\" href=\"{base}{STYLE_PATH}\">\
<script src=\"{base}{SHELL_PATH}\"></script><script src=\"{base}{SCRIPT_PATH}\"></script></head>\
<body><div id=\"lectern\" class=\"fui-app fui-client\"><div class=\"fui-client-col\">{topbar}\
<main class=\"lc-main\"><div class=\"lc-container\">\
<div class=\"lc-header\"><h3>{title}</h3><p class=\"lc-subtitle\">{subtitle}</p></div>{cards}\
</div></main></div></div></body></html>",
        lang = t.locale(),
        mode = mode(cookies),
        palette = palette(cookies),
        base = escape(base),
        topbar = topbar(base, &t, None),
        subtitle = escape(&t.t("ui_subtitle")),
    )
}

/// A page of a book inside Lectern's frame: the top bar, and the page itself in an iframe below it.
///
/// `src` is the address of the page (the iframe loads the same address; the server answers that request with the page
/// alone). `books` are all the books, `current` the one being read.
#[must_use]
pub fn framed_page(
    base: &str,
    books: &[Arc<Book>],
    current: &Book,
    src: &str,
    cookies: Option<&str>,
    accept_language: Option<&str>,
) -> String {
    let t = Texts::new(pick_locale(cookies, accept_language));
    format!(
        "<!doctype html>\n<html lang=\"{lang}\" data-fui-theme=\"{mode}\" data-fui-palette=\"{palette}\">\
<head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
<title>{title} · Lectern</title>\
<link rel=\"icon\" type=\"image/png\" href=\"{base}{ICON_PATH}\">\
<link rel=\"stylesheet\" href=\"{base}{STYLE_PATH}\">\
<script src=\"{base}{SHELL_PATH}\"></script><script src=\"{base}{SCRIPT_PATH}\"></script>\
<script src=\"{base}{FRAME_PATH}\" defer></script></head>\
<body class=\"lc-framed\"><div id=\"lectern\" class=\"fui-app fui-client\"><div class=\"fui-client-col\">{topbar}\
<main class=\"lc-frame\"><iframe id=\"book-frame\" class=\"lc-book-frame\" src=\"{src}\" title=\"{title}\"></iframe></main>\
</div></div></body></html>",
        lang = t.locale(),
        mode = mode(cookies),
        palette = palette(cookies),
        base = escape(base),
        title = escape(&current.title),
        src = escape(src),
        topbar = topbar(base, &t, Some((books, &current.name))),
    )
}
