//! The tables, as typed functions. Every statement is parameterised; nothing from a request is ever put into SQL text.

pub mod attachments;
pub mod audit;
pub mod deliveries;
pub mod mailboxes;
pub mod messages;
pub mod settings;
pub mod uploads;

use crate::error::Result;
use quench_pg::Pool;

/// The database.
#[derive(Debug, Clone)]
pub struct Store {
    pub pool: Pool,
}

impl Store {
    /// Connects with the schema on the search path (Foundry installed the tables there).
    ///
    /// # Errors
    ///
    /// When the database cannot be reached.
    pub async fn connect(url: &str, schema: &str) -> Result<Self> {
        if !schema
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
            || schema.is_empty()
        {
            return Err(crate::error::Error::Internal(format!(
                "bad schema name {schema:?}"
            )));
        }
        let mut config = quench_pg::Config::from_url(url)?;
        config.options = Some(format!("-c search_path={schema}"));
        let pool = Pool::connect_with(config, quench_pg::PoolOptions::default()).await?;
        Ok(Self { pool })
    }

    /// A cheap round trip, for the readiness probe.
    ///
    /// # Errors
    ///
    /// When the database does not answer.
    pub async fn ping(&self) -> Result<()> {
        self.pool.query_one("SELECT 1", &[]).await?;
        Ok(())
    }
}
