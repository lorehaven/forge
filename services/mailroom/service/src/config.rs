//! Where things live and how the service behaves, from the environment.

use std::time::Duration;

/// A transport the first start writes into the settings (so a deployment is ready to send without anyone opening the panel).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeedTransport {
    pub host: String,
    pub port: u16,
    pub security: String,
    pub username: String,
    pub password: String,
    pub tls_roots_pem: String,
}

/// Settings read once at start.
#[derive(Debug, Clone)]
pub struct Config {
    /// `DATABASE_URL` (or `POSTGRES_URL`).
    pub database_url: String,
    /// `DB_SCHEMA` (default `mailroom`): where Foundry installed the tables.
    pub schema: String,
    /// `MAILROOM_VAULT_KEY`: the secret the stored passwords are sealed under. At least 32 characters.
    pub vault_key: String,
    /// `MAILROOM_VAULT_KEY_PREVIOUS`: the key before a rotation; accepted for reading and replaced as secrets are used.
    pub vault_key_previous: Option<String>,
    /// `MAILROOM_ALLOW_EPHEMERAL_KEY`: with no vault key, make one up for this run (development only).
    pub allow_ephemeral_key: bool,
    /// `MAILROOM_SYNC_SECS`: how often a mailbox is looked at (default 120).
    pub sync_interval: Duration,
    /// `MAILROOM_INITIAL_MESSAGES`: newest messages kept per folder on the first look (default 300).
    pub initial_messages: u32,
    /// `MAILROOM_BACKGROUND`: run the sync and send workers (default true; tests turn them off).
    pub background: bool,
    /// `MAILROOM_PRIVATE_HOSTS`: hosts allowed to be private addresses, comma separated; added to the administrator's list.
    pub private_hosts: Vec<String>,
    /// `MAILROOM_EXTRA_PORTS`: ports allowed besides the mail ports, comma separated.
    pub extra_ports: Vec<u16>,
    /// The transport written into the settings the first time (`MAILROOM_SMTP_*`).
    pub seed_transport: Option<SeedTransport>,
    /// `MAILROOM_FROM`: the default sender identity, `Name <address>`, written the first time.
    pub seed_identity: Option<String>,
    /// `MAILROOM_TRUSTED_ORIGINS`: extra origins (scheme://host) whose browsers may call the API with a session cookie.
    pub trusted_origins: Vec<String>,
}

impl Config {
    /// Reads the environment.
    #[must_use]
    pub fn from_env() -> Self {
        let text = |name: &str| quench_core::env::get_or(name, "").trim().to_string();
        let list = |name: &str| -> Vec<String> {
            text(name)
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect()
        };
        let seed_transport = (!text("MAILROOM_SMTP_HOST").is_empty()).then(|| SeedTransport {
            host: text("MAILROOM_SMTP_HOST"),
            port: quench_core::env::get_parsed("MAILROOM_SMTP_PORT", 587),
            security: {
                let s = text("MAILROOM_SMTP_SECURITY");
                if s.is_empty() {
                    "starttls".to_string()
                } else {
                    s
                }
            },
            username: text("MAILROOM_SMTP_USERNAME"),
            password: quench_core::env::get_or("MAILROOM_SMTP_PASSWORD", ""),
            tls_roots_pem: quench_core::env::get_or("MAILROOM_SMTP_TLS_ROOTS_PEM", ""),
        });
        Self {
            database_url: quench_core::env::get_any(&["DATABASE_URL", "POSTGRES_URL"], ""),
            schema: quench_core::env::get_or("DB_SCHEMA", "mailroom"),
            vault_key: quench_core::env::get_or("MAILROOM_VAULT_KEY", ""),
            vault_key_previous: Some(quench_core::env::get_or("MAILROOM_VAULT_KEY_PREVIOUS", ""))
                .filter(|k| !k.is_empty()),
            allow_ephemeral_key: quench_core::env::is_or("MAILROOM_ALLOW_EPHEMERAL_KEY", false),
            sync_interval: Duration::from_secs(quench_core::env::get_parsed(
                "MAILROOM_SYNC_SECS",
                120,
            )),
            initial_messages: quench_core::env::get_parsed("MAILROOM_INITIAL_MESSAGES", 300),
            background: quench_core::env::is_or("MAILROOM_BACKGROUND", true),
            private_hosts: list("MAILROOM_PRIVATE_HOSTS"),
            extra_ports: list("MAILROOM_EXTRA_PORTS")
                .iter()
                .filter_map(|p| p.parse().ok())
                .collect(),
            seed_transport,
            seed_identity: Some(text("MAILROOM_FROM")).filter(|f| !f.is_empty()),
            trusted_origins: list("MAILROOM_TRUSTED_ORIGINS"),
        }
    }

    /// A config for tests: no background work, an ephemeral key.
    #[must_use]
    pub fn for_tests(schema: &str, database_url: &str) -> Self {
        Self {
            database_url: database_url.to_string(),
            schema: schema.to_string(),
            vault_key: "test-vault-key-0123456789abcdef0123456789".to_string(),
            vault_key_previous: None,
            allow_ephemeral_key: false,
            sync_interval: Duration::from_secs(120),
            initial_messages: 50,
            background: false,
            private_hosts: vec!["127.0.0.1".to_string(), "localhost".to_string()],
            extra_ports: Vec::new(),
            seed_transport: None,
            seed_identity: None,
            trusted_origins: Vec::new(),
        }
    }
}
