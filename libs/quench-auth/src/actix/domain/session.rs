//! Realm sessions.
//!
//! Sessions live in the cache store (Redis in a deployment, in-process for
//! tests), not in Postgres. They are ephemeral by nature: expiry is the store's
//! TTL, revocation is a delete, and there is nothing to sweep. Refresh-token
//! rotation is a single atomic take, so a replayed token cannot win a race.
//!
//! Two keys per session:
//!
//! ```text
//! session:{sid}         -> { username, refresh_hash }   TTL = refresh lifetime
//! refresh:{token_hash}  -> sid                          TTL = refresh lifetime
//! ```

use quench_cache::CacheStore;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use uuid::Uuid;

/// A session, as much of it as anything outside this module needs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub username: String,
}

pub struct SessionDb {
    store: CacheStore,
}

impl SessionDb {
    pub fn init(store: CacheStore) -> Arc<Self> {
        if store.is_shared() {
            tracing::info!("sessions are held in {}", store.topology());
        } else {
            tracing::warn!(
                "sessions are held in-process: a second replica will not see them, \
                 and they are lost on restart. Set REDIS_URL for a shared store."
            );
        }
        Arc::new(Self { store })
    }

    /// Reads `REDIS_URL`/`CACHE_URL`, falling back to an in-process store.
    pub async fn from_env() -> anyhow::Result<Arc<Self>> {
        let store = CacheStore::from_env("forge-session").await?;
        Ok(Self::init(store))
    }

    fn session_key(id: &str) -> String {
        format!("session:{id}")
    }

    fn refresh_key(token_hash: &str) -> String {
        format!("refresh:{token_hash}")
    }

    pub async fn create(&self, username: &str, ttl_secs: i64) -> anyhow::Result<(Session, String)> {
        let refresh_token = new_refresh_token();
        let refresh_hash = hash_refresh_token(&refresh_token);
        let session = Session {
            id: Uuid::new_v4().to_string(),
            username: username.to_string(),
        };
        let ttl = ttl_secs.max(1) as u64;

        self.store
            .set(
                &Self::session_key(&session.id),
                json!({ "username": username, "refresh_hash": refresh_hash }),
                Some(ttl),
            )
            .await?;
        self.store
            .set(
                &Self::refresh_key(&refresh_hash),
                json!(session.id),
                Some(ttl),
            )
            .await?;

        tracing::info!("created session {} for {username}", session.id);
        Ok((session, refresh_token))
    }

    /// Exchanges a refresh token for a new one.
    ///
    /// The old token is consumed atomically, so a token presented twice - by a
    /// racing client or by an attacker replaying a stolen one - succeeds at
    /// most once.
    pub async fn rotate(
        &self,
        refresh_token: &str,
        ttl_secs: i64,
    ) -> anyhow::Result<Option<(Session, String)>> {
        let old_hash = hash_refresh_token(refresh_token);
        let Some(session_id) = self.store.take(&Self::refresh_key(&old_hash)).await? else {
            return Ok(None);
        };
        let Some(session_id) = session_id.as_str().map(str::to_string) else {
            return Ok(None);
        };

        let Some(record) = self.store.get(&Self::session_key(&session_id)).await? else {
            // The session went away between the two reads: expired or revoked.
            return Ok(None);
        };
        let Some(username) = record.get("username").and_then(|v| v.as_str()) else {
            return Ok(None);
        };

        let next_token = new_refresh_token();
        let next_hash = hash_refresh_token(&next_token);
        let ttl = ttl_secs.max(1) as u64;

        self.store
            .set(
                &Self::session_key(&session_id),
                json!({ "username": username, "refresh_hash": next_hash }),
                Some(ttl),
            )
            .await?;
        self.store
            .set(&Self::refresh_key(&next_hash), json!(session_id), Some(ttl))
            .await?;

        Ok(Some((
            Session {
                id: session_id,
                username: username.to_string(),
            },
            next_token,
        )))
    }

    /// Ends the session behind a refresh token. Takes effect everywhere at
    /// once, because every service reads the same store.
    pub async fn revoke_by_refresh_token(&self, refresh_token: &str) -> anyhow::Result<bool> {
        let hash = hash_refresh_token(refresh_token);
        let Some(session_id) = self.store.take(&Self::refresh_key(&hash)).await? else {
            return Ok(false);
        };
        if let Some(id) = session_id.as_str() {
            self.store.remove(&Self::session_key(id)).await?;
            tracing::info!("revoked session {id}");
        }
        Ok(true)
    }

    /// Ends a session by id, provided it belongs to `username`.
    pub async fn revoke(&self, id: &str, username: &str) -> anyhow::Result<bool> {
        let Some(record) = self.store.get(&Self::session_key(id)).await? else {
            return Ok(false);
        };
        if record.get("username").and_then(|v| v.as_str()) != Some(username) {
            return Ok(false);
        }

        if let Some(hash) = record.get("refresh_hash").and_then(|v| v.as_str()) {
            self.store.remove(&Self::refresh_key(hash)).await?;
        }
        self.store.remove(&Self::session_key(id)).await?;
        tracing::info!("revoked session {id}");
        Ok(true)
    }

    /// Whether the session is still usable. Called on every authenticated
    /// request, so it is a single point read.
    pub async fn is_active(&self, id: &str, username: &str) -> anyhow::Result<bool> {
        let Some(record) = self.store.get(&Self::session_key(id)).await? else {
            tracing::debug!("session {id} is gone (expired or revoked)");
            return Ok(false);
        };
        Ok(record.get("username").and_then(|v| v.as_str()) == Some(username))
    }
}

fn new_refresh_token() -> String {
    format!("{}.{}", Uuid::new_v4(), Uuid::new_v4())
}

fn hash_refresh_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}
