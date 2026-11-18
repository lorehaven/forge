//! What every handler and worker shares.

use crate::config::Config;
use crate::limits::Limits;
use crate::settings::Stored;
use crate::store::Store;
use std::sync::{Arc, RwLock};

#[derive(Debug, Clone)]
pub struct App {
    pub config: Arc<Config>,
    pub store: Store,
    pub settings: Arc<RwLock<Stored>>,
    pub limits: Arc<Limits>,
    /// Held while the settings are read, changed and written back, so two administrators cannot undo each other.
    pub settings_lock: Arc<tokio::sync::Mutex<()>>,
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

    /// The address of a user: `name@<domain>`.
    #[must_use]
    pub fn address_of(&self, name: &str) -> String {
        format!("{}@{}", name.to_ascii_lowercase(), self.config.domain)
    }
}
