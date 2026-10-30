//! Authorization codes: the short-lived, single-use handoff between
//! `GET /authorize` and `POST /token` in the PKCE flow. See `api/oauth.rs`.

use quench_core::time::Time;
use quench_db::prelude::Model;
use quench_serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorizationCodeRow {
    pub code_hash: String,
    pub client_id: String,
    pub username: String,
    pub redirect_uri: String,
    pub scope: String,
    pub pkce_challenge: String,
    pub created_at: Time,
    pub expires_at: Time,
    pub consumed_at: Option<Time>,
}

impl Model for AuthorizationCodeRow {
    fn table_name() -> String {
        format!(
            "{}.authorization_codes",
            quench_auth::prelude::realm::auth_schema()
        )
    }

    fn columns() -> Vec<&'static str> {
        vec![
            "code_hash",
            "client_id",
            "username",
            "redirect_uri",
            "scope",
            "pkce_challenge",
            "created_at",
            "expires_at",
            "consumed_at",
        ]
    }

    fn primary_key_name() -> String {
        "code_hash".to_string()
    }
}

impl AuthorizationCodeRow {
    #[must_use]
    pub fn is_usable(&self, now: Time) -> bool {
        self.consumed_at.is_none() && self.expires_at > now
    }
}
