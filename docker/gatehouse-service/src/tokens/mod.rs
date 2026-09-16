//! Single-use email-verification tokens; `purpose` in the cache key stops cross-purpose redemption.

use quench_cache::CacheStore;
use serde_json::json;
use uuid::Uuid;

pub struct VerificationTokens {
    store: CacheStore,
}

impl VerificationTokens {
    /// Reads `REDIS_URL`/`CACHE_URL`, falling back to in-process (dev mode).
    pub async fn from_env() -> anyhow::Result<Self> {
        let store = CacheStore::from_env("forge-verify").await?;
        Ok(Self { store })
    }

    /// An always-in-process store, for tests - no cache backend required.
    pub fn in_memory() -> Self {
        Self {
            store: CacheStore::in_memory(),
        }
    }

    fn key(purpose: &str, token: &str) -> String {
        format!("{purpose}:{token}")
    }

    /// Mints a token for `username`, valid for `ttl_secs` - the token itself
    /// is the credential, so it must travel only to the address being verified.
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

    /// Redeems once, atomically (`GETDEL`) - a racing double-click or replay
    /// succeeds at most once.
    pub async fn redeem(&self, purpose: &str, token: &str) -> anyhow::Result<Option<String>> {
        let value = self.store.take(&Self::key(purpose, token)).await?;
        Ok(value.and_then(|value| value.as_str().map(str::to_string)))
    }
}

pub const PURPOSE_VERIFY_EMAIL: &str = "verify-email";
pub const PURPOSE_RESET_PASSWORD: &str = "reset-password";
