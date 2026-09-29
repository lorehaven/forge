//! Rate limits for the endpoints that cause an email to be sent.
//!
//! Each request costs a real email and the mail relay has a small daily
//! allowance, so anyone able to trigger sends freely can both flood a victim's
//! inbox and use up the estate's quota. A fixed-window counter per subject
//! (client address, account name, email address) keeps that bounded.
//!
//! The counter is a *set* of unique hits per window, not a number: adding to a
//! set is atomic (Redis `SADD`), while read-increment-write over a number lets
//! parallel requests all read the same value and all pass. Every request adds
//! its hit first and then counts, so a burst can be counted too strictly but
//! never too loosely - at most `max` requests of a window are allowed.

mod client_ip;

pub use client_ip::ClientIp;

use quench_cache::CacheStore;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

/// At most `max` hits per `window_secs`-long window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limit {
    pub max: usize,
    pub window_secs: u64,
}

impl Limit {
    pub const fn new(max: usize, window_secs: u64) -> Self {
        Self { max, window_secs }
    }

    pub const fn per_minute(max: usize) -> Self {
        Self::new(max, 60)
    }

    pub const fn per_hour(max: usize) -> Self {
        Self::new(max, 60 * 60)
    }

    pub const fn per_day(max: usize) -> Self {
        Self::new(max, 24 * 60 * 60)
    }
}

/// The limits in force. Deliberately not configurable per deployment: they
/// exist to protect the mail quota, and a knob invites turning it off.
pub mod policy {
    use super::Limit;

    /// Registration: per client, and per address so one victim cannot be
    /// mail-bombed by registering many usernames with their email.
    pub const REGISTER_IP: Limit = Limit::per_hour(20);
    pub const REGISTER_EMAIL: Limit = Limit::per_day(3);

    /// Password reset: per client and per submitted username.
    pub const RESET_IP: Limit = Limit::per_hour(20);
    pub const RESET_USER: Limit = Limit::per_hour(3);

    /// Resending a verification link: per client, per username, and no more
    /// than one a minute for the same username.
    pub const RESEND_IP: Limit = Limit::per_hour(20);
    pub const RESEND_USER: Limit = Limit::per_hour(3);
    pub const RESEND_COOLDOWN: Limit = Limit::per_minute(1);

    /// Asking to change an email address: per account, per client, and per
    /// destination address so one victim's inbox cannot be used as a target.
    pub const EMAIL_CHANGE_USER: Limit = Limit::per_hour(3);
    pub const EMAIL_CHANGE_IP: Limit = Limit::per_hour(20);
    pub const EMAIL_CHANGE_ADDRESS: Limit = Limit::per_day(3);

    /// A service's notification requests for one person, and for one kind of
    /// them: a failing pipeline must not turn into a stream of mail.
    pub const NOTIFY_USER: Limit = Limit::per_hour(20);
    pub const NOTIFY_TEMPLATE: Limit = Limit::per_hour(10);

    /// Default cap on emails sent by the whole estate per day - a little under
    /// the relay's free allowance (300).
    pub const DEFAULT_DAILY_MAIL: usize = 250;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Allowed,
    /// Over the limit; the window ends in `retry_after_secs`.
    Limited {
        retry_after_secs: u64,
    },
}

impl Verdict {
    pub fn is_allowed(self) -> bool {
        self == Verdict::Allowed
    }
}

type Clock = Arc<dyn Fn() -> u64 + Send + Sync>;

#[derive(Clone)]
pub struct RateLimiter {
    store: CacheStore,
    clock: Clock,
}

impl RateLimiter {
    pub fn new(store: CacheStore) -> Self {
        Self::with_clock(store, Arc::new(unix_now))
    }

    /// `REDIS_URL`/`CACHE_URL`, else in-process (dev mode) - same as the token
    /// and session stores, so limits are shared by every replica in a cluster.
    pub async fn from_env() -> anyhow::Result<Self> {
        Ok(Self::new(CacheStore::from_env("forge-ratelimit").await?))
    }

    /// In-process, for tests.
    pub fn in_memory() -> Self {
        Self::new(CacheStore::in_memory())
    }

    /// A limiter whose idea of "now" (unix seconds) is supplied - tests move it
    /// forward instead of sleeping through a window.
    pub fn with_clock(store: CacheStore, clock: Clock) -> Self {
        Self { store, clock }
    }

    /// Counts one hit by `subject` under `scope` and says whether it is within
    /// `limit`. Blocked hits count too, so hammering does not earn a way in
    /// before the window ends. If the counter store fails the answer is
    /// `Limited`: the estate's sessions depend on the same store, so it being
    /// down is already an outage, and failing open would switch the limits off
    /// exactly when something is wrong.
    pub async fn check(&self, scope: &str, subject: &str, limit: Limit) -> Verdict {
        match self.try_check(scope, subject, limit).await {
            Ok(verdict) => verdict,
            Err(err) => {
                tracing::error!("rate limit store failed for {scope}, refusing: {err}");
                Verdict::Limited {
                    retry_after_secs: limit.window_secs,
                }
            }
        }
    }

    async fn try_check(
        &self,
        scope: &str,
        subject: &str,
        limit: Limit,
    ) -> Result<Verdict, quench_cache::CacheError> {
        let now = (self.clock)();
        let window = limit.window_secs.max(1);
        let index = now / window;
        let retry_after_secs = (index + 1) * window - now;
        let key = format!("ratelimit:{scope}:{}:{index}", fingerprint(subject));

        // Already far over: refuse without growing the set, so a flood cannot
        // make one key arbitrarily large.
        let held = self.store.set_members(&key).await?.len();
        if held >= limit.max.saturating_mul(2).max(1) {
            return Ok(Verdict::Limited { retry_after_secs });
        }

        self.store
            .add_to_set(&key, &Uuid::new_v4().to_string(), Some(window * 2))
            .await?;
        let count = self.store.set_members(&key).await?.len();
        Ok(if count <= limit.max {
            Verdict::Allowed
        } else {
            Verdict::Limited { retry_after_secs }
        })
    }

    /// All of `checks` must pass. Every one is counted even after a failure -
    /// each dimension reflects the traffic it actually saw.
    pub async fn check_all(&self, checks: &[(&str, &str, Limit)]) -> Verdict {
        let mut worst = Verdict::Allowed;
        for (scope, subject, limit) in checks {
            if let Verdict::Limited { retry_after_secs } = self.check(scope, subject, *limit).await
            {
                worst = match worst {
                    Verdict::Limited {
                        retry_after_secs: had,
                    } => Verdict::Limited {
                        retry_after_secs: had.max(retry_after_secs),
                    },
                    Verdict::Allowed => Verdict::Limited { retry_after_secs },
                };
            }
        }
        worst
    }
}

/// The subject as a bounded, case-insensitive key: whatever a user typed
/// (length, characters) cannot bloat or inject into the cache key space.
fn fingerprint(subject: &str) -> String {
    let normalized = subject.trim().to_lowercase();
    let digest = Sha256::digest(normalized.as_bytes());
    hex::encode(&digest[..16])
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
