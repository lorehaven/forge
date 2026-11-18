//! The administrator's settings, one JSON document, and the sealed transport password.

use super::Store;
use crate::error::Result;

impl Store {
    pub async fn setting_get(&self, key: &str) -> Result<Option<String>> {
        let row = self
            .pool
            .query_opt("SELECT value FROM settings WHERE key = $1", &[&key])
            .await?;
        row.map(|r| Ok(r.get("value")?)).transpose()
    }

    pub async fn setting_put(&self, key: &str, value: &str) -> Result<()> {
        self.pool
            .execute(
                "INSERT INTO settings (key, value) VALUES ($1, $2) ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value",
                &[&key, &value],
            )
            .await?;
        Ok(())
    }

    /// Writes only when nothing is there (the first-start seed); returns whether it wrote.
    pub async fn setting_put_if_absent(&self, key: &str, value: &str) -> Result<bool> {
        Ok(self
            .pool
            .execute(
                "INSERT INTO settings (key, value) VALUES ($1, $2) ON CONFLICT (key) DO NOTHING",
                &[&key, &value],
            )
            .await?
            > 0)
    }
}
