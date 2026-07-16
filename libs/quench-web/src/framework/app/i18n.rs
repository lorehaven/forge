use fluent_syntax::ast::{Entry, Expression, InlineExpression, PatternElement};
use fluent_syntax::parser::parse;
use std::collections::HashSet;
use std::path::Path;

pub fn available_locales() -> anyhow::Result<Vec<String>> {
    let i18n_path = Path::new("i18n");
    let mut locales = Vec::new();

    if !i18n_path.exists() {
        return Ok(Vec::new());
    }

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

pub fn parse_ftl_with_options(supported_locales: Option<&[String]>) -> anyhow::Result<String> {
    let i18n_path = Path::new("i18n");
    if !i18n_path.exists() {
        return Ok("{}".to_string());
    }

    let allowed: Option<HashSet<&str>> =
        supported_locales.map(|v| v.iter().map(|s| s.as_str()).collect());
    let mut all_locales = serde_json::Map::new();

    for entry in std::fs::read_dir(i18n_path)? {
        let entry = entry?;
        let path = entry.path();

        if path.extension().map(|s| s == "ftl").unwrap_or(false) {
            let locale = path.file_stem().unwrap().to_string_lossy().to_string();
            if let Some(allowed) = &allowed
                && !allowed.contains(locale.as_str())
            {
                continue;
            }

            let ftl_string = std::fs::read_to_string(&path)?;
            let res = parse(&*ftl_string)
                .map_err(|_| anyhow::anyhow!("Failed to parse FTL: {}", path.display()))?;
            let mut map = serde_json::Map::new();

            for entry in res.body.iter() {
                if let Entry::Message(msg) = entry
                    && let Some(pattern) = &msg.value
                {
                    let val = pattern
                        .elements
                        .iter()
                        .map(|e| match e {
                            PatternElement::TextElement { value: t } => t.to_string(),
                            // Keep variable placeables as `{$name}` tokens so the
                            // client can substitute them via data-i18n-args.
                            PatternElement::Placeable {
                                expression:
                                    Expression::Inline(InlineExpression::VariableReference { id }),
                            } => format!("{{${}}}", id.name),
                            _ => String::new(),
                        })
                        .collect::<String>();

                    map.insert(msg.id.name.to_string(), serde_json::json!(val));
                }
            }

            all_locales.insert(locale, serde_json::json!(map));
        }
    }

    Ok(serde_json::to_string_pretty(&all_locales)?)
}

pub fn generate_translations_js(supported_locales: &[String]) -> String {
    let translations = parse_ftl_with_options(Some(supported_locales)).unwrap_or_default();
    format!("window.qTranslations = {translations};")
}
