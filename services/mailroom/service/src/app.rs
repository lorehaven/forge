//! What every handler and worker shares.

use crate::config::Config;
use crate::limits::Limits;
use crate::settings::Stored;
use crate::store::Store;
use crate::sync::SyncHub;
use crate::vault::Vault;
use mail_client::netguard::Policy;
use std::sync::{Arc, RwLock};

#[derive(Debug, Clone)]
pub struct App {
    pub config: Arc<Config>,
    pub store: Store,
    pub vault: Arc<Vault>,
    pub settings: Arc<RwLock<Stored>>,
    pub limits: Arc<Limits>,
    pub hub: Arc<SyncHub>,
    /// Held while the settings are read, changed and written back, so two administrators cannot undo each other.
    pub settings_lock: Arc<tokio::sync::Mutex<()>>,
    /// Wakes the sender when something is queued, so mail does not wait for the next poll.
    pub wake: Arc<tokio::sync::Notify>,
}

impl App {
    /// The current settings (a copy).
    #[must_use]
    pub fn settings(&self) -> Stored {
        self.settings
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Replaces the cached settings.
    pub fn set_settings(&self, settings: Stored) {
        *self
            .settings
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = settings;
    }

    /// Where Mailroom may connect: the deployment's allowances and the administrator's.
    #[must_use]
    pub fn policy(&self) -> Policy {
        let settings = self.settings();
        let mut private_hosts = self.config.private_hosts.clone();
        private_hosts.extend(settings.private_hosts);
        let mut extra_ports = self.config.extra_ports.clone();
        extra_ports.extend(settings.extra_ports);
        Policy {
            private_hosts,
            extra_ports,
            connect_timeout: Some(std::time::Duration::from_secs(15)),
        }
    }
}
