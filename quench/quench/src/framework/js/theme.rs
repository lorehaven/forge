use crate::Theme;
use strum::IntoEnumIterator;

pub fn theme_js(default_theme: &str) -> String {
    let themes = Theme::iter().collect::<Vec<_>>();
    theme_js_with_options(default_theme, &themes)
}

pub fn theme_js_with_options(default_theme: &str, supported_themes: &[Theme]) -> String {
    let themes = supported_themes
        .iter()
        .map(|theme| {
            let theme_str = theme.to_string();
            format!("\"{theme_str}\": \"/assets/css/themes/{theme_str}.css\"")
        })
        .collect::<Vec<_>>()
        .join(",\n");

    format!(
        r#"// ---- Theme Configuration ----
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
    if (THEMES[theme]) {{
        link.href = THEMES[theme];
    }}
}}

function updateTheme(newTheme) {{
    if (!THEMES[newTheme]) return;
    setCookie(THEME_COOKIE, newTheme);
    applyTheme(newTheme);
    window.dispatchEvent(new Event("themeChanged"));
}}

// Polling for cookie changes
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

// On page load
document.addEventListener("DOMContentLoaded", () => {{
    currentTheme = getTheme();
    applyTheme(currentTheme);
    watchThemeChanges();
}});

    "#
    )
    .trim()
    .to_string()
}
