//! Application tokens: long-lived, revocable secrets a person mints for their
//! own tools and exchanges at `POST /token` (`grant_type=app_token`) for a
//! short-lived access token. See `api/app_tokens.rs`.

use quench_core::time::Time;
use quench_db::prelude::Model;
use quench_serde::{Deserialize, Serialize};

/// Prefix on every minted token, so a leaked one is recognisable in a scan.
pub const TOKEN_PREFIX: &str = "lht_";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppTokenRow {
    pub token_hash: String,
    pub id: String,
    pub username: String,
    pub name: String,
    /// Space separated `service:action` entries; empty means the owner's full access.
    pub scope: String,
    pub created_at: Time,
    pub expires_at: Option<Time>,
    pub last_used_at: Option<Time>,
    pub revoked_at: Option<Time>,
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
    pub fn is_usable(&self, now: Time) -> bool {
        self.revoked_at.is_none() && self.expires_at.is_none_or(|expires| expires > now)
    }

    /// The scope entries, empty when the token carries the owner's full access.
    #[must_use]
    pub fn entries(&self) -> Vec<String> {
        self.scope.split_whitespace().map(str::to_string).collect()
    }
}
