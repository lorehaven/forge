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

function interpolate(value, el) {{
    const argsAttr = el.getAttribute("data-i18n-args");
    if (!argsAttr) return value;
    try {{
        const args = JSON.parse(argsAttr);
        for (const name of Object.keys(args)) {{
            value = value.split("{{$" + name + "}}").join(args[name]);
        }}
    }} catch (e) {{ /* leave value untranslated on malformed args */ }}
    return value;
}}

function translate(key, fallback) {{
    const dict = getTranslations()[getLocale()] || {{}};
    return dict[key] !== undefined ? dict[key] : (fallback !== undefined ? fallback : key);
}}

let applyingTranslations = false;

function applyTranslations(locale) {{
    const translations = getTranslations();
    const dict = translations[locale];
    if (!dict) return;

    applyingTranslations = true;
    try {{
        document.querySelectorAll("[data-i18n]").forEach(el => {{
            const key = el.getAttribute("data-i18n");
            if (dict[key] !== undefined) {{
                el.textContent = interpolate(dict[key], el);
            }}
        }});

        document.querySelectorAll("[data-i18n-placeholder]").forEach(el => {{
            const key = el.getAttribute("data-i18n-placeholder");
            if (dict[key] !== undefined) {{
                el.placeholder = interpolate(dict[key], el);
            }}
        }});

        document.querySelectorAll("[data-i18n-title]").forEach(el => {{
            const key = el.getAttribute("data-i18n-title");
            if (dict[key] !== undefined) {{
                el.title = interpolate(dict[key], el);
            }}
        }});
    }} finally {{
        applyingTranslations = false;
    }}
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

    // Translate content inserted outside of htmx swaps (e.g. SSE fragments).
    // Only element insertions that carry data-i18n markers trigger a re-apply,
    // so the text mutations applyTranslations makes never re-trigger it.
    let mutationScheduled = false;
    const observer = new MutationObserver((mutations) => {{
        if (applyingTranslations || mutationScheduled) return;
        const needsApply = mutations.some(m => Array.from(m.addedNodes).some(n =>
            n.nodeType === 1 && (
                n.hasAttribute("data-i18n") || n.hasAttribute("data-i18n-placeholder") ||
                n.hasAttribute("data-i18n-title") ||
                n.querySelector("[data-i18n], [data-i18n-placeholder], [data-i18n-title]")
            )
        ));
        if (!needsApply) return;
        mutationScheduled = true;
        requestAnimationFrame(() => {{
            mutationScheduled = false;
            applyTranslations(getLocale());
        }});
    }});
    observer.observe(document.body, {{ childList: true, subtree: true }});
}});

document.addEventListener("htmx:afterSwap", () => {{
    currentLocale = getLocale();
    applyTranslations(currentLocale);
}});

window.qUpdateI18n = () => applyTranslations(getLocale());
window.qT = translate;
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
