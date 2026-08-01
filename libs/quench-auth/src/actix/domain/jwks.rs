//! Verifies tokens against gatehouse's published JWKS, instead of a shared
//! secret every service used to hold. Every relying party's [`JwtConfig`]
//! (`crate::actix::domain::jwt`) uses one of these.

use crate::actix::domain::jwt::KeyResolver;
use async_trait::async_trait;
use jsonwebtoken::DecodingKey;
use quench_cache::CacheStore;
use serde::Deserialize;
use std::time::Duration;

const POSITIVE_TTL_SECS: u64 = 3600;
/// Short, so a `kid` gatehouse issues moments after a failed lookup is not
/// stuck looking unknown for the rest of the negative-cache window - but long
/// enough that a forged/garbage `kid` cannot turn into a request-per-call
/// flood against gatehouse.
const NEGATIVE_TTL_SECS: u64 = 30;

#[derive(Debug, Deserialize)]
struct JwkSet {
    keys: Vec<Jwk>,
}

#[derive(Debug, Deserialize)]
struct Jwk {
    kid: String,
    /// Base64url raw Ed25519 public key - the estate only ever publishes
    /// `OKP`/`Ed25519` keys, so `kty`/`crv` aren't worth parsing.
    x: String,
}

pub struct JwksVerifier {
    http: reqwest::Client,
    jwks_url: String,
    cache: CacheStore,
}

impl JwksVerifier {
    pub async fn from_env() -> anyhow::Result<Self> {
        let base = envmnt::get_or("GATEHOUSE_URL", "");
        anyhow::ensure!(
            !base.is_empty(),
            "GATEHOUSE_URL must be set - every service verifies realm tokens against gatehouse's JWKS"
        );
        let tls_verify = envmnt::get_or("GATEHOUSE_TLS_VERIFY", "true")
            .parse()
            .unwrap_or(true);
        Self::new(&base, tls_verify).await
    }

    pub async fn new(gatehouse_base_url: &str, tls_verify: bool) -> anyhow::Result<Self> {
        let http = reqwest::Client::builder()
            .danger_accept_invalid_certs(!tls_verify)
            .timeout(Duration::from_secs(5))
            .build()?;
        let cache = CacheStore::from_env("jwks").await?;
        Ok(Self {
            http,
            jwks_url: format!(
                "{}/.well-known/jwks.json",
                gatehouse_base_url.trim_end_matches('/')
            ),
            cache,
        })
    }

    fn cache_key(kid: &str) -> String {
        format!("kid:{kid}")
    }

    /// Refetches the whole set (cheap, and a rotation publishes more than one
    /// key at once), caching every key it finds, then answers for `kid`.
    async fn refetch(&self, kid: &str) -> Option<String> {
        let response = self.http.get(&self.jwks_url).send().await.ok()?;
        let set: JwkSet = response.json().await.ok()?;

        let mut found = None;
        for jwk in set.keys {
            let _ = self
                .cache
                .set(
                    &Self::cache_key(&jwk.kid),
                    serde_json::Value::String(jwk.x.clone()),
                    Some(POSITIVE_TTL_SECS),
                )
                .await;
            if jwk.kid == kid {
                found = Some(jwk.x);
            }
        }
        if found.is_none() {
            let _ = self
                .cache
                .set(
                    &Self::cache_key(kid),
                    serde_json::Value::Null,
                    Some(NEGATIVE_TTL_SECS),
                )
                .await;
        }
        found
    }
}

#[async_trait]
impl KeyResolver for JwksVerifier {
    async fn resolve(&self, kid: &str) -> Option<DecodingKey> {
        let cached = self.cache.get(&Self::cache_key(kid)).await.ok().flatten();
        let x = match cached {
            Some(serde_json::Value::String(x)) => Some(x),
            Some(serde_json::Value::Null) => return None, // cached miss
            _ => self.refetch(kid).await,
        }?;
        DecodingKey::from_ed_components(&x).ok()
    }
}
