use fluent_syntax::ast::{Entry, PatternElement};
use fluent_syntax::parser::parse;
use std::collections::HashSet;
use std::path::Path;

pub fn available_locales() -> anyhow::Result<Vec<String>> {
    let i18n_path = Path::new("i18n");
    let mut locales = Vec::new();

    for entry in std::fs::read_dir(i18n_path)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().map(|s| s == "ftl").unwrap_or(false)
            && let Some(locale) = path.file_stem().map(|s| s.to_string_lossy().to_string())
        {
            locales.push(locale);
        }
    }

    locales.sort();
    locales.dedup();
    Ok(locales)
}

pub fn validate_locales_exist(locales: &[String]) -> anyhow::Result<()> {
    for locale in locales {
        let path = Path::new("i18n").join(format!("{locale}.ftl"));
        if !path.exists() {
            anyhow::bail!("missing locale file: {}", path.display());
        }
    }
    Ok(())
}

pub fn parse_ftl() -> anyhow::Result<String> {
    parse_ftl_with_options(None)
}

pub fn parse_ftl_with_options(supported_locales: Option<&[String]>) -> anyhow::Result<String> {
    let i18n_path = Path::new("i18n");
    let allowed: Option<HashSet<&str>> =
        supported_locales.map(|v| v.iter().map(|s| s.as_str()).collect());

    // This will hold all locales
    let mut all_locales = serde_json::Map::new();

    for entry in std::fs::read_dir(i18n_path)? {
        let entry = entry?;
        let path = entry.path();

        if path.extension().map(|s| s == "ftl").unwrap_or(false) {
            // Locale name = file stem (e.g., "en-US.ftl" -> "en-US")
            let locale = path.file_stem().unwrap().to_string_lossy().to_string();
            if let Some(allowed) = &allowed
                && !allowed.contains(locale.as_str())
            {
                continue;
            }

            // Read and parse FTL
            let ftl_string = std::fs::read_to_string(&path)?;
            let res = parse(&*ftl_string)
                .map_err(|_| anyhow::anyhow!("Failed to parse FTL: {}", path.display()))?;

            // Build JSON for this locale
            let mut map = serde_json::Map::new();

            // Extract messages
            for entry in res.body.iter() {
                if let Entry::Message(msg) = entry
                    && let Some(pattern) = &msg.value
                {
                    // Flatten PatternElements to simple string
                    let val = pattern
                        .elements
                        .iter()
                        .map(|e| match e {
                            PatternElement::TextElement { value: t } => t,
                            _ => "", // Ignore variables/complex patterns for now
                        })
                        .collect::<String>();

                    map.insert(msg.id.name.to_string(), serde_json::json!(val));
                }
            }

            // Add to the combined locales map
            all_locales.insert(locale, serde_json::json!(map));
        }
    }

    // Serialize the joint JSON string for all locales
    let joint_json = serde_json::to_string_pretty(&all_locales)?;
    Ok(joint_json)
}

pub fn locale_js() -> String {
    let locales = available_locales().unwrap_or_default();
    locale_js_with_options(&locales, None)
}

pub fn locale_js_with_options(
    supported_locales: &[String],
    default_locale: Option<&str>,
) -> String {
    let resolved_default_locale = match default_locale {
        Some(locale) if supported_locales.iter().any(|l| l == locale) => locale.to_string(),
        _ => supported_locales
            .first()
            .cloned()
            .unwrap_or_else(|| "en-US".to_string()),
    };

    format!(
        r#"
// ---- Configuration ----

const DEFAULT_LOCALE = "{resolved_default_locale}";
const COOKIE_NAME = "qlocale";

// Example translations
const TRANSLATIONS = {};

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

    if (!locale || !TRANSLATIONS[locale]) {{
        locale = DEFAULT_LOCALE;
        setCookie(COOKIE_NAME, locale);
    }}

    return locale;
}}

function applyTranslations(locale) {{
    const dict = TRANSLATIONS[locale];
    if (!dict) return;

    document.querySelectorAll("[data-i18n]").forEach(el => {{
        const key = el.getAttribute("data-i18n");
        if (dict[key]) {{
            el.textContent = dict[key];
        }}
    }});
}}

function updateLocale(newLocale) {{
    if (!TRANSLATIONS[newLocale]) return;

    setCookie(COOKIE_NAME, newLocale);
    applyTranslations(newLocale);

    // Notify listeners
    window.dispatchEvent(new Event("localeChanged"));
}}

// ---- Cookie Change Detection ----

// Browsers don't emit cookie change events,
// so we poll for changes.

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

// ---- On Page Load ----

document.addEventListener("DOMContentLoaded", () => {{
    currentLocale = getLocale();
    applyTranslations(currentLocale);
    watchLocaleChanges();
}});

// Expose for manual switching
window.setLocale = updateLocale;
    "#,
        parse_ftl_with_options(Some(supported_locales)).unwrap_or_default()
    )
    .trim()
    .to_string()
}
