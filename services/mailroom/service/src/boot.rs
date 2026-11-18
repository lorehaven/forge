//! Starting up: the vault, the database, the first-start settings.

use crate::app::App;
use crate::config::Config;
use crate::error::{Error, Result};
use crate::limits::Limits;
use crate::settings::{self, Stored, StoredTransport};
use crate::store::Store;
use crate::sync::SyncHub;
use crate::vault::Vault;
use mailroom_core::admin::Identity;
use std::sync::{Arc, RwLock};

/// Builds the vault from the configuration.
///
/// # Errors
///
/// When there is no key (and an ephemeral one is not allowed) or it is too short.
pub fn vault(config: &Config) -> Result<Vault> {
    if config.vault_key.is_empty() {
        if config.allow_ephemeral_key {
            quench_log::warn!(
                "MAILROOM_VAULT_KEY is not set: using a key made for this run; stored passwords will not survive a restart"
            );
            return Ok(Vault::ephemeral());
        }
        return Err(Error::Internal(
            "MAILROOM_VAULT_KEY is not set: it seals the passwords of the mailboxes (32 characters at least)".into(),
        ));
    }
    Vault::new(&config.vault_key, config.vault_key_previous.as_deref()).map_err(Error::from)
}

/// Opens the database and loads the settings, writing the deployment's seed the first time.
///
/// # Errors
///
/// A failure to reach the database or a bad configuration.
pub async fn app(config: Config) -> Result<App> {
    if config.database_url.trim().is_empty() {
        return Err(Error::Internal(
            "no database configured: set DATABASE_URL (or POSTGRES_URL); the tables are installed by Foundry".into(),
        ));
    }
    let vault = vault(&config)?;
    let store = Store::connect(&config.database_url, &config.schema).await?;
    seed(&store, &vault, &config).await?;
    let loaded = settings::load(&store).await?;
    Ok(App {
        config: Arc::new(config),
        store,
        vault: Arc::new(vault),
        settings: Arc::new(RwLock::new(loaded)),
        limits: Arc::new(Limits::default()),
        hub: Arc::new(SyncHub::new()),
        settings_lock: Arc::new(tokio::sync::Mutex::new(())),
        wake: Arc::new(tokio::sync::Notify::new()),
    })
}

/// The first start: the deployment's sending server and sender address become settings. Later starts leave them alone, so
/// what an administrator changed in the panel stays.
async fn seed(store: &Store, vault: &Vault, config: &Config) -> Result<()> {
    let mut seeded = Stored::default();
    if let Some(t) = &config.seed_transport {
        seeded.transport = Some(StoredTransport {
            host: t.host.to_ascii_lowercase(),
            port: t.port,
            security: t.security.clone(),
            username: t.username.clone(),
            tls_roots_pem: t.tls_roots_pem.clone(),
        });
    }
    if let Some(from) = &config.seed_identity {
        let (name, address) = match from.rsplit_once('<') {
            Some((name, rest)) => (
                name.trim().trim_matches('"').to_string(),
                rest.trim_end_matches('>').trim().to_string(),
            ),
            None => (String::new(), from.trim().to_string()),
        };
        if mailroom_core::draft::plausible_address(&address) {
            seeded.identities.push(Identity {
                address,
                name,
                default: true,
            });
        }
    }
    if store
        .setting_put_if_absent("main", &quench_serde::json::to_string(&seeded))
        .await?
        && let Some(t) = &config.seed_transport
        && !t.password.is_empty()
    {
        settings::save_transport_secret(store, vault, &t.password).await?;
    }
    Ok(())
}
