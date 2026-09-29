//! Who wants which notifications.
//!
//! A row exists only where someone's choice differs from the template's default,
//! so changing a default later reaches everyone who never touched it.

use super::catalog::{self, Template};
use chrono::{DateTime, Utc};
use quench_db::prelude::{Crud, Db, DbError, Model};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct PrefRow {
    /// `username|template` - the repository wants one key column.
    pub key: String,
    pub username: String,
    pub template: String,
    pub enabled: bool,
    pub updated_at: DateTime<Utc>,
}

impl Model for PrefRow {
    fn table_name() -> String {
        format!(
            "{}.notification_prefs",
            quench_auth::prelude::realm::auth_schema()
        )
    }

    fn columns() -> Vec<&'static str> {
        vec!["key", "username", "template", "enabled", "updated_at"]
    }

    fn primary_key_name() -> String {
        "key".to_string()
    }
}

fn key(username: &str, template: &str) -> String {
    format!("{username}|{template}")
}

pub struct Preferences<'a> {
    db: &'a Db,
}

impl<'a> Preferences<'a> {
    pub fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// The choices this user made, by template id.
    pub async fn overrides(&self, username: &str) -> Result<HashMap<String, bool>, DbError> {
        let rows = self
            .db
            .repository::<PrefRow>()
            .find_by("username", username)
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| (row.template, row.enabled))
            .collect())
    }

    pub async fn is_subscribed(
        &self,
        username: &str,
        template: &Template,
    ) -> Result<bool, DbError> {
        self.wants(username, template, false).await
    }

    /// `requested`: the caller holds a subscription the person made, which
    /// stands in for the default - only an explicit opt-out of this kind beats it.
    pub async fn wants(
        &self,
        username: &str,
        template: &Template,
        requested: bool,
    ) -> Result<bool, DbError> {
        let row = self
            .db
            .repository::<PrefRow>()
            .read(&key(username, template.id))
            .await?;
        Ok(row.map_or(template.default_on || requested, |row| row.enabled))
    }

    /// Every kind, with whether this user gets it.
    pub async fn effective(
        &self,
        username: &str,
    ) -> Result<Vec<(&'static Template, bool)>, DbError> {
        let chosen = self.overrides(username).await?;
        Ok(catalog::all()
            .iter()
            .map(|t| (t, chosen.get(t.id).copied().unwrap_or(t.default_on)))
            .collect())
    }

    /// Records the choice - or forgets it when it just matches the default.
    pub async fn set(
        &self,
        username: &str,
        template: &Template,
        enabled: bool,
    ) -> Result<(), DbError> {
        let repo = self.db.repository::<PrefRow>();
        let key = key(username, template.id);
        let existing = repo.read(&key).await?;
        if enabled == template.default_on {
            if existing.is_some() {
                repo.delete(&key).await?;
            }
            return Ok(());
        }
        let row = PrefRow {
            key,
            username: username.to_string(),
            template: template.id.to_string(),
            enabled,
            updated_at: Utc::now(),
        };
        if existing.is_some() {
            repo.update(&row).await?;
        } else {
            repo.create(&row).await?;
        }
        Ok(())
    }
}
