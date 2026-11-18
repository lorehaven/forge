//! The administrator's settings as stored, and the defaults.

use crate::error::{Error, Result};
use crate::store::Store;
use crate::vault::{TRANSPORT_CONTEXT, Vault};
use mailroom_core::admin::{Identity, Settings, Transport};
use quench_serde::{Deserialize, Serialize};

const KEY: &str = "main";
const SECRET_KEY: &str = "transport_secret";

/// The outgoing server, without its password (that is sealed under its own key).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct StoredTransport {
    pub host: String,
    pub port: u16,
    pub security: String,
    pub username: String,
    #[serde(default)]
    pub tls_roots_pem: String,
}

/// The document kept in the `settings` table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stored {
    #[serde(default)]
    pub transport: Option<StoredTransport>,
    #[serde(default)]
    pub identities: Vec<Identity>,
    #[serde(default)]
    pub private_hosts: Vec<String>,
    #[serde(default)]
    pub extra_ports: Vec<u16>,
    pub daily_budget: u32,
    pub hourly_per_source: u32,
    pub max_accounts_per_user: u32,
    pub sync_interval_secs: u32,
}

impl Default for Stored {
    fn default() -> Self {
        Self {
            transport: None,
            identities: Vec::new(),
            private_hosts: Vec::new(),
            extra_ports: Vec::new(),
            daily_budget: 500,
            hourly_per_source: 120,
            max_accounts_per_user: 10,
            sync_interval_secs: 120,
        }
    }
}

impl Stored {
    /// What the API shows.
    #[must_use]
    pub fn public(&self, has_password: bool) -> Settings {
        Settings {
            transport: self.transport.as_ref().map(|t| Transport {
                host: t.host.clone(),
                port: t.port,
                security: t.security.clone(),
                username: t.username.clone(),
                has_password,
                custom_certificate: !t.tls_roots_pem.is_empty(),
            }),
            identities: self.identities.clone(),
            private_hosts: self.private_hosts.clone(),
            extra_ports: self.extra_ports.clone(),
            daily_budget: self.daily_budget,
            hourly_per_source: self.hourly_per_source,
            max_accounts_per_user: self.max_accounts_per_user,
            sync_interval_secs: self.sync_interval_secs,
        }
    }

    /// The sender identity for `address` (case-insensitive), or the default when `address` is empty.
    #[must_use]
    pub fn identity(&self, address: &str) -> Option<&Identity> {
        let address = address.trim();
        if address.is_empty() {
            return self
                .identities
                .iter()
                .find(|i| i.default)
                .or_else(|| self.identities.first());
        }
        self.identities
            .iter()
            .find(|i| i.address.eq_ignore_ascii_case(address))
    }
}

/// Reads the settings (the defaults when nothing is stored).
///
/// # Errors
///
/// A database failure, or a stored document that cannot be read.
pub async fn load(store: &Store) -> Result<Stored> {
    match store.setting_get(KEY).await? {
        Some(text) => quench_serde::json::from_str(&text)
            .map_err(|e| Error::Internal(format!("stored settings are unreadable: {e}"))),
        None => Ok(Stored::default()),
    }
}

/// Stores the settings.
///
/// # Errors
///
/// A database failure.
pub async fn save(store: &Store, settings: &Stored) -> Result<()> {
    store
        .setting_put(KEY, &quench_serde::json::to_string(settings))
        .await
}

/// The sealed transport password, if there is one.
///
/// # Errors
///
/// A database failure.
pub async fn transport_secret(store: &Store) -> Result<Option<String>> {
    store.setting_get(SECRET_KEY).await
}

/// Stores a transport password, sealed.
///
/// # Errors
///
/// A database failure.
pub async fn save_transport_secret(store: &Store, vault: &Vault, password: &str) -> Result<()> {
    store
        .setting_put(SECRET_KEY, &vault.seal(TRANSPORT_CONTEXT, password))
        .await
}

/// Opens the transport password.
///
/// # Errors
///
/// When none is stored or it cannot be opened.
pub async fn open_transport_secret(store: &Store, vault: &Vault) -> Result<String> {
    let sealed = transport_secret(store)
        .await?
        .ok_or_else(|| Error::Upstream("no sending server is configured".into()))?;
    Ok(vault.open(TRANSPORT_CONTEXT, &sealed)?.0)
}
