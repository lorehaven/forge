//! Where things live and how the service behaves, from the environment.

use std::time::Duration;

/// Settings read once at start.
#[derive(Debug, Clone)]
pub struct Config {
    /// `DATABASE_URL` (or `POSTGRES_URL`).
    pub database_url: String,
    /// `DB_SCHEMA` (default `mailroom`): where Foundry installed the tables.
    pub schema: String,
    /// `MAILROOM_DOMAIN`: the part of every address after the `@` (default `mailroom.local`).
    pub domain: String,
    /// `MAILROOM_BACKGROUND`: run the maintenance worker (default true; tests turn it off).
    pub background: bool,
    /// `MAILROOM_MAINTENANCE_SECS`: how often the maintenance pass runs (default 3600).
    pub maintenance_interval: Duration,
    /// `MAILROOM_TRUSTED_ORIGINS`: extra origins (scheme://host) whose browsers may call the API with a session cookie.
    pub trusted_origins: Vec<String>,
}

/// A domain that is safe to put after an `@`: lowercase letters, digits, dots and hyphens, with a dot or `localhost`.
#[must_use]
pub fn domain_ok(domain: &str) -> bool {
    !domain.is_empty()
        && domain.len() <= 253
        && domain
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '-'))
        && !domain.starts_with(['.', '-'])
        && !domain.ends_with(['.', '-'])
        && !domain.contains("..")
        && (domain.contains('.') || domain == "localhost")
}

impl Config {
    /// Reads the environment.
    ///
    /// # Panics
    ///
    /// When `MAILROOM_DOMAIN` is not a usable domain: every address would be wrong.
    #[must_use]
    pub fn from_env() -> Self {
        let text = |name: &str| quench_core::env::get_or(name, "").trim().to_string();
        let domain = Some(text("MAILROOM_DOMAIN"))
            .filter(|d| !d.is_empty())
            .unwrap_or_else(|| "mailroom.local".to_string())
            .to_ascii_lowercase();
        assert!(
            domain_ok(&domain),
            "MAILROOM_DOMAIN {domain:?} is not a domain"
        );
        Self {
            database_url: quench_core::env::get_any(&["DATABASE_URL", "POSTGRES_URL"], ""),
            schema: quench_core::env::get_or("DB_SCHEMA", "mailroom"),
            domain,
            background: quench_core::env::is_or("MAILROOM_BACKGROUND", true),
            maintenance_interval: Duration::from_secs(
                quench_core::env::get_parsed("MAILROOM_MAINTENANCE_SECS", 3600).max(1),
            ),
            trusted_origins: text("MAILROOM_TRUSTED_ORIGINS")
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect(),
        }
    }

    /// A config for tests: no background work.
    #[must_use]
    pub fn for_tests(schema: &str, database_url: &str) -> Self {
        Self {
            database_url: database_url.to_string(),
            schema: schema.to_string(),
            domain: "home.example".to_string(),
            background: false,
            maintenance_interval: Duration::from_secs(3600),
            trusted_origins: Vec::new(),
        }
    }
}
