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

/// Watches the realm session and sends the browser to login when it ends.
///
/// Every page in the estate is server-rendered behind an authentication check,
/// which settles the question once, at render time, and never again. A tab left
/// open then goes on looking signed in for as long as nobody touches it - past
/// the session's expiry, and past a logout performed in another tab or by an
/// administrator revoking it. The first anyone learns of it is a click that
/// lands on a login page, having lost whatever was on screen.
///
/// Deliberately narrow about what counts as evidence. Only a well-formed answer
/// saying `authenticated: false` redirects. A 404 - a service that serves no
/// `/ui/status` - a network blip, or a body that will not parse are all left
/// alone: throwing somebody out of a working page because one request failed
/// would be a worse bug than the one this closes.
pub fn session_script(resources_prefix: &str, interval_secs: u64) -> Script {
    let status_url = format!("{resources_prefix}/status");
    let login_url = format!("{resources_prefix}/login");
    let interval_ms = interval_secs * 1000;

    js!(
        r#"
// ---- Session Watch ----

const SESSION_STATUS_URL = "{status_url}";
const SESSION_LOGIN_URL = "{login_url}";
const SESSION_INTERVAL_MS = {interval_ms};

let sessionCheckInFlight = false;

async function checkSession() {{
    // A hidden tab is not showing anybody stale state, and polling from every
    // background tab multiplies the cost by however many are open.
    if (sessionCheckInFlight || document.hidden) return;

    // The login page redirecting to itself would be a navigation loop, once a
    // minute, forever.
    if (window.location.pathname.replace(/\/$/, "").endsWith("/login")) return;

    sessionCheckInFlight = true;
    try {{
        const response = await fetch(SESSION_STATUS_URL, {{
            credentials: "same-origin",
            headers: {{ "Accept": "application/json" }},
            cache: "no-store",
        }});
        if (!response.ok) return;

        const status = await response.json();
        // `=== false` rather than falsy: a body without the field says nothing
        // about the session, and must not be read as a denial.
        if (status && status.authenticated === false) {{
            window.location.href = SESSION_LOGIN_URL;
        }}
    }} catch (error) {{
        // Offline, or a service restarting. Not evidence the session ended.
    }} finally {{
        sessionCheckInFlight = false;
    }}
}}

setInterval(checkSession, SESSION_INTERVAL_MS);

// The case the interval alone is bad at: a tab left for an hour and come back
// to. Checking on the way in means the redirect happens before the first click
// rather than because of it.
document.addEventListener("visibilitychange", () => {{
    if (!document.hidden) checkSession();
}});
        "#,
        status_url = status_url,
        login_url = login_url,
        interval_ms = interval_ms
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
