//! `${name}` substitution.
//!
//! Every string that reaches a command line goes through here. A name that is
//! not defined is an error rather than an empty string: a blank `DATABASE_URL`
//! fails much further from its cause than a config error does.

use anyhow::{Context, Result, bail};
use std::collections::BTreeMap;
use std::path::Path;

use crate::config::Var;

/// The names in scope for one expansion: the config's `[vars]`, plus whatever
/// the caller adds for this particular service, task or warning.
pub struct Scope<'a> {
    globals: &'a BTreeMap<String, String>,
    locals: Vec<(String, String)>,
}

impl<'a> Scope<'a> {
    pub fn new(globals: &'a BTreeMap<String, String>) -> Self {
        Self {
            globals,
            locals: Vec::new(),
        }
    }

    #[must_use]
    pub fn with(mut self, name: &str, value: impl Into<String>) -> Self {
        self.locals.push((name.to_string(), value.into()));
        self
    }

    fn lookup(&self, name: &str) -> Option<&str> {
        // Locals last-wins, so a service's `name` beats a global of the same
        // name rather than the other way round.
        self.locals
            .iter()
            .rev()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
            .or_else(|| self.globals.get(name).map(String::as_str))
    }

    pub fn expand(&self, template: &str) -> Result<String> {
        let mut out = String::with_capacity(template.len());
        let mut rest = template;

        while let Some(start) = rest.find("${") {
            out.push_str(&rest[..start]);
            let after = &rest[start + 2..];
            let Some(end) = after.find('}') else {
                bail!("unterminated `${{` in \"{template}\"");
            };
            let name = &after[..end];
            let Some(value) = self.lookup(name) else {
                bail!("unknown variable `${{{name}}}` in \"{template}\"");
            };
            out.push_str(value);
            rest = &after[end + 1..];
        }

        out.push_str(rest);
        Ok(out)
    }

    pub fn expand_all(&self, templates: &[String]) -> Result<Vec<String>> {
        templates.iter().map(|t| self.expand(t)).collect()
    }

    pub fn expand_map(&self, map: &BTreeMap<String, String>) -> Result<Vec<(String, String)>> {
        map.iter()
            .map(|(key, value)| Ok((key.clone(), self.expand(value)?)))
            .collect()
    }
}

/// Resolves `[vars]` once, at load. Entries cannot refer to one another - one
/// pass, no ordering to reason about, and nothing that can loop.
pub fn resolve(root: &Path, vars: &BTreeMap<String, Var>) -> Result<BTreeMap<String, String>> {
    let mut resolved = BTreeMap::new();

    for (name, var) in vars {
        let value = match var {
            Var::Literal(literal) => literal.clone(),
            Var::EnvFile {
                env_file,
                key,
                default,
            } => env_file_value(&root.join(env_file), key)
                .with_context(|| format!("resolving var `{name}`"))?
                .unwrap_or_else(|| default.clone()),
        };
        resolved.insert(name.clone(), value);
    }

    Ok(resolved)
}

/// First `KEY=value` in a dotenv file, unquoted. A missing file is not an
/// error - that is what the var's `default` is for.
fn env_file_value(path: &Path, key: &str) -> Result<Option<String>> {
    if !path.is_file() {
        return Ok(None);
    }

    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let prefix = format!("{key}=");

    Ok(text
        .lines()
        .find_map(|line| line.strip_prefix(&prefix))
        .map(|value| {
            value
                .trim()
                .trim_matches('"')
                .trim_matches('\'')
                .to_string()
        }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn globals() -> BTreeMap<String, String> {
        BTreeMap::from([("host".to_string(), "localhost".to_string())])
    }

    #[test]
    fn expands_globals_and_locals() {
        let globals = globals();
        let scope = Scope::new(&globals).with("port", "8443");
        assert_eq!(
            scope.expand("https://${host}:${port}/health").unwrap(),
            "https://localhost:8443/health"
        );
    }

    #[test]
    fn locals_win_over_globals() {
        let globals = globals();
        let scope = Scope::new(&globals).with("host", "127.0.0.1");
        assert_eq!(scope.expand("${host}").unwrap(), "127.0.0.1");
    }

    #[test]
    fn a_lone_dollar_is_literal() {
        let globals = globals();
        let scope = Scope::new(&globals);
        assert_eq!(
            scope.expand("costs $5, not ${host}").unwrap(),
            "costs $5, not localhost"
        );
    }

    #[test]
    fn unknown_names_are_an_error() {
        let globals = globals();
        let scope = Scope::new(&globals);
        assert!(
            scope
                .expand("${nope}")
                .unwrap_err()
                .to_string()
                .contains("nope")
        );
    }

    #[test]
    fn unterminated_braces_are_an_error() {
        let globals = globals();
        assert!(Scope::new(&globals).expand("${host").is_err());
    }

    #[test]
    fn reads_a_quoted_dotenv_value() {
        let dir = std::env::temp_dir().join("foreman-vars-test");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join(".env");
        std::fs::write(&file, "OTHER=1\nJWT_SECRET=\"sh h\"\nJWT_SECRET=later\n").unwrap();

        assert_eq!(
            env_file_value(&file, "JWT_SECRET").unwrap().as_deref(),
            Some("sh h")
        );
        assert_eq!(env_file_value(&file, "MISSING").unwrap(), None);
        std::fs::remove_dir_all(&dir).ok();
    }
}
