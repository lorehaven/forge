//! The administrator's settings as stored, and the defaults.

use crate::error::{Error, Result};
use crate::store::Store;
use mailroom_core::admin::Settings;
use quench_serde::{Deserialize, Serialize};

const KEY: &str = "main";

/// The document kept in the `settings` table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stored {
    pub daily_budget: u32,
    pub hourly_per_source: u32,
    pub hourly_per_person: u32,
    pub quota_mb: u32,
}

impl Default for Stored {
    fn default() -> Self {
        Self {
            daily_budget: 2000,
            hourly_per_source: 600,
            hourly_per_person: 120,
            quota_mb: 500,
        }
    }
}

impl Stored {
    /// What the API shows.
    #[must_use]
    pub fn public(&self, domain: &str) -> Settings {
        Settings {
            domain: domain.to_string(),
            daily_budget: self.daily_budget,
            hourly_per_source: self.hourly_per_source,
            hourly_per_person: self.hourly_per_person,
            quota_mb: self.quota_mb,
        }
    }

    /// What one mailbox may hold, in bytes.
    #[must_use]
    pub fn quota_bytes(&self) -> i64 {
        i64::from(self.quota_mb) * 1024 * 1024
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
