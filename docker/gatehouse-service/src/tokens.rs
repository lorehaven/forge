//! Single-use, time-limited tokens that prove control of an email address.
//!
//! Registration and password reset both need exactly this shape - mint a
//! random token, email a link carrying it, and redeem it once - so this is
//! the one place that logic lives rather than two near-identical copies. It
//! sits on the same cache store `SessionDb` uses (`quench-cache`), under its
//! own key prefix, for the same reason a refresh token lives there rather
//! than in Postgres: expiry is the store's TTL, and there is nothing to sweep.
//!
//! `purpose` is folded into the key rather than trusted from the caller: a
//! verification link is not a password reset link, and a token minted for one
//! must not redeem as the other, even though both are otherwise identical
//! random strings. Getting this wrong would mean a leaked "confirm your
//! email" link could reset the account's password instead.

use quench_cache::CacheStore;
use serde_json::json;
use uuid::Uuid;

pub struct VerificationTokens {
    store: CacheStore,
}

impl VerificationTokens {
    /// Reads `REDIS_URL`/`CACHE_URL`, falling back to an in-process store -
    /// same as `SessionDb::from_env`, and for the same reason: a single dev
    /// process needs no shared store, and a real deployment needs one across
    /// replicas.
    pub async fn from_env() -> anyhow::Result<Self> {
        let store = CacheStore::from_env("forge-verify").await?;
        Ok(Self { store })
    }

    fn key(purpose: &str, token: &str) -> String {
        format!("{purpose}:{token}")
    }

    /// Mints a token for `username`, valid for `ttl_secs`. The token itself is
    /// the credential - nothing about the recipient is encoded in it - so it
    /// has to travel only to the address being verified.
    pub async fn issue(
        &self,
        purpose: &str,
        username: &str,
        ttl_secs: u64,
    ) -> anyhow::Result<String> {
        let token = Uuid::new_v4().to_string();
        self.store
            .set(&Self::key(purpose, &token), json!(username), Some(ttl_secs))
            .await?;
        Ok(token)
    }

    /// Redeems a token for `purpose`, or `None` if it never existed, already
    /// was redeemed, or expired. Atomic (`GETDEL` under Redis), so a token
    /// presented twice - a racing double-click, or a replay - succeeds at
    /// most once.
    pub async fn redeem(&self, purpose: &str, token: &str) -> anyhow::Result<Option<String>> {
        let value = self.store.take(&Self::key(purpose, token)).await?;
        Ok(value.and_then(|value| value.as_str().map(str::to_string)))
    }
}

pub const PURPOSE_VERIFY_EMAIL: &str = "verify-email";
pub const PURPOSE_RESET_PASSWORD: &str = "reset-password";
