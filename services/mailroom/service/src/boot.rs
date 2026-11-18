//! Starting up: the database and the settings.

use crate::app::App;
use crate::config::Config;
use crate::error::{Error, Result};
use crate::limits::Limits;
use crate::settings;
use crate::store::Store;
use std::sync::{Arc, RwLock};

/// Opens the database and loads the settings.
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
    let store = Store::connect(&config.database_url, &config.schema).await?;
    let loaded = settings::load(&store).await?;
    Ok(App {
        config: Arc::new(config),
        store,
        settings: Arc::new(RwLock::new(loaded)),
        limits: Arc::new(Limits::default()),
        settings_lock: Arc::new(tokio::sync::Mutex::new(())),
    })
}
