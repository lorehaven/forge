use crate::{Script, Theme, js};

pub fn locale_script(supported_locales: &[String], default_locale: Option<&str>) -> Script {
    let resolved_default_locale = match default_locale {
        Some(locale) if supported_locales.iter().any(|l| l == locale) => locale.to_string(),
        _ => supported_locales
            .first()
            .cloned()
            .unwrap_or_else(|| "en-US".to_string()),
    };

    js!(
        r#"
// ---- Configuration ----

const DEFAULT_LOCALE = "{resolved_default_locale}";
const COOKIE_NAME = "qlocale";
const getTranslations = () => window.qTranslations || {{}};

// ---- Cookie Utilities ----

function getCookie(name) {{
    const value = `; ${{document.cookie}}`;
    const parts = value.split(`; ${{name}}=`);
    if (parts.length === 2) {{
        return parts.pop().split(";").shift();
    }}
    return null;
}}

function setCookie(name, value, days = 365) {{
    const expires = new Date();
    expires.setTime(expires.getTime() + (days * 24 * 60 * 60 * 1000));
    document.cookie = `${{name}}=${{value}}; expires=${{expires.toUTCString()}}; path=/`;
}}

// ---- Locale Logic ----

function getLocale() {{
    let locale = getCookie(COOKIE_NAME);
    const translations = getTranslations();

    if (!locale || !translations[locale]) {{
        locale = DEFAULT_LOCALE;
        setCookie(COOKIE_NAME, locale);
    }}

    return locale;
}}

function applyTranslations(locale) {{
    const translations = getTranslations();
    const dict = translations[locale];
    if (!dict) return;

    document.querySelectorAll("[data-i18n]").forEach(el => {{
        const key = el.getAttribute("data-i18n");
        if (dict[key]) {{
            el.textContent = dict[key];
        }}
    }});

    document.querySelectorAll("[data-i18n-placeholder]").forEach(el => {{
        const key = el.getAttribute("data-i18n-placeholder");
        if (dict[key]) {{
            el.placeholder = dict[key];
        }}
    }});
}}

function updateLocale(newLocale) {{
    const translations = getTranslations();
    if (!translations[newLocale]) return;

    setCookie(COOKIE_NAME, newLocale);
    applyTranslations(newLocale);
    window.dispatchEvent(new Event("localeChanged"));
}}

let currentLocale = null;

function watchLocaleChanges() {{
    setInterval(() => {{
        const locale = getCookie(COOKIE_NAME);
        if (locale !== currentLocale) {{
            currentLocale = locale;
            applyTranslations(locale);
        }}
    }}, 500);
}}

document.addEventListener("DOMContentLoaded", () => {{
    currentLocale = getLocale();
    applyTranslations(currentLocale);
    watchLocaleChanges();
}});

document.addEventListener("htmx:afterSwap", () => {{
    currentLocale = getLocale();
    applyTranslations(currentLocale);
}});

window.qUpdateI18n = () => applyTranslations(getLocale());
window.setLocale = updateLocale;
window.getLocale = getLocale;
        "#,
        resolved_default_locale = resolved_default_locale
    )
}

pub fn theme_script(
    default_theme: &str,
    supported_themes: &[Theme],
    resources_prefix: &str,
) -> Script {
    let themes = supported_themes
        .iter()
        .map(|theme| {
            let theme_str = theme.to_string();
            format!("\"{theme_str}\": \"{resources_prefix}/assets/css/themes/{theme_str}.css\"")
        })
        .collect::<Vec<_>>()
        .join(",\n");

    js!(
        r#"
// ---- Theme Configuration ----
const DEFAULT_THEME = "{default_theme}";
const THEME_COOKIE = "qtheme";
const THEMES = {{
{themes}
}};

function getTheme() {{
    let theme = getCookie(THEME_COOKIE);
    if (!theme || !THEMES[theme]) {{
        theme = DEFAULT_THEME;
        setCookie(THEME_COOKIE, theme);
    }}
    return theme;
}}

function applyTheme(theme) {{
    const linkId = "theme-link";
    let link = document.getElementById(linkId);
    if (!link) {{
        link = document.createElement("link");
        link.id = linkId;
        link.rel = "stylesheet";
        document.head.appendChild(link);
    }}
    const targetHref = THEMES[theme];
    if (!targetHref) return;

    if (link.getAttribute("href") === targetHref) return;
    link.href = targetHref;
}}

function updateTheme(newTheme) {{
    if (!THEMES[newTheme]) return;
    setCookie(THEME_COOKIE, newTheme);
    applyTheme(newTheme);
    window.dispatchEvent(new Event("themeChanged"));
}}

let currentTheme = null;
function watchThemeChanges() {{
    setInterval(() => {{
        const theme = getCookie(THEME_COOKIE);
        if (theme !== currentTheme) {{
            currentTheme = theme;
            applyTheme(theme);
        }}
    }}, 500);
}}

currentTheme = getTheme();
applyTheme(currentTheme);
watchThemeChanges();
        "#,
        default_theme = default_theme,
        themes = themes
    )
}
