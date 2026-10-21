//! Application tokens: long-lived, revocable secrets a person mints for their
//! own tools and exchanges at `POST /token` (`grant_type=app_token`) for a
//! short-lived access token. See `api/app_tokens.rs`.

use chrono::{DateTime, Utc};
use quench_db::prelude::Model;
use serde::{Deserialize, Serialize};

/// Prefix on every minted token, so a leaked one is recognisable in a scan.
pub const TOKEN_PREFIX: &str = "lht_";

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AppTokenRow {
    pub token_hash: String,
    pub id: String,
    pub username: String,
    pub name: String,
    /// Space separated `service:action` entries; empty means the owner's full access.
    pub scope: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
}

impl Model for AppTokenRow {
    fn table_name() -> String {
        format!("{}.app_tokens", quench_auth::prelude::realm::auth_schema())
    }

    fn columns() -> Vec<&'static str> {
        vec![
            "token_hash",
            "id",
            "username",
            "name",
            "scope",
            "created_at",
            "expires_at",
            "last_used_at",
            "revoked_at",
        ]
    }

    fn primary_key_name() -> String {
        "token_hash".to_string()
    }
}

impl AppTokenRow {
    #[must_use]
    pub fn is_usable(&self, now: DateTime<Utc>) -> bool {
        self.revoked_at.is_none() && self.expires_at.is_none_or(|expires| expires > now)
    }

    /// The scope entries, empty when the token carries the owner's full access.
    #[must_use]
    pub fn entries(&self) -> Vec<String> {
        self.scope.split_whitespace().map(str::to_string).collect()
    }
}
