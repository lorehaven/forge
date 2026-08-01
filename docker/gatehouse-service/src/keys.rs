//! Gatehouse's own Ed25519 signing keys.
//!
//! Generated at boot if none exist, held decrypted in memory (refreshed from
//! Postgres at load and after rotation) so every sign or verify is a local
//! lookup, never a database or network round trip on the request path -
//! relying parties fetch the public half over `/.well-known/jwks.json`
//! (`quench_auth::actix::domain::jwks::JwksVerifier` on their side); gatehouse
//! resolves its own tokens directly through this same struct, implementing
//! both `KeyResolver` and `KeySigner`.
//!
//! Private keys are encrypted at rest with a key derived (via SHA-256, so any
//! passphrase-shaped string works) from `GATEHOUSE_KEY_ENCRYPTION_KEY`.

use async_trait::async_trait;
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use chrono::{DateTime, Utc};
use jsonwebtoken::{DecodingKey, EncodingKey};
use quench_auth::actix::domain::jwt::{KeyResolver, KeySigner};
use quench_auth::actix::domain::signing::{decoding_key, encoding_key, generate_signing_key};
use quench_db::prelude::{Crud, Db, Model, Repository};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
struct SigningKeyRow {
    kid: String,
    algorithm: String,
    /// Hex-encoded `nonce || ChaCha20-Poly1305(private_key_der)`. Hex rather
    /// than raw bytea: quench-db's generic `Crud` path round-trips a model
    /// through `jsonb_populate_record`, which does not turn a JSON
    /// array-of-numbers (serde's default `Vec<u8>` encoding) into bytea - it
    /// silently stores the array's own text instead. A hex `TEXT` column
    /// sidesteps that entirely.
    private_key: String,
    public_key: String,
    created_at: DateTime<Utc>,
    not_after: Option<DateTime<Utc>>,
}

impl Model for SigningKeyRow {
    fn table_name() -> String {
        format!("{}.signing_keys", quench_auth::prelude::realm::auth_schema())
    }

    fn columns() -> Vec<&'static str> {
        vec![
            "kid",
            "algorithm",
            "private_key",
            "public_key",
            "created_at",
            "not_after",
        ]
    }

    fn primary_key_name() -> String {
        "kid".to_string()
    }
}

/// A key with its private half already decrypted, held only in memory.
struct LoadedKey {
    kid: String,
    private_key_der: Vec<u8>,
    public_key: Vec<u8>,
    not_after: Option<DateTime<Utc>>,
}

pub struct SigningKeys {
    repo: Repository<SigningKeyRow>,
    cipher: ChaCha20Poly1305,
    /// Outstanding tokens signed by a retired key must keep verifying for the
    /// rest of their own TTL, so a rotated-out key stays published in JWKS
    /// until this much time has passed.
    retire_after_secs: i64,
    keys: RwLock<Vec<LoadedKey>>,
}

impl SigningKeys {
    pub async fn init(db: Db, retire_after_secs: i64) -> anyhow::Result<std::sync::Arc<Self>> {
        let cipher = build_cipher()?;
        let this = std::sync::Arc::new(Self {
            repo: db.repository::<SigningKeyRow>(),
            cipher,
            retire_after_secs,
            keys: RwLock::new(Vec::new()),
        });
        this.reload().await?;
        if this.keys.read().unwrap().iter().all(|k| k.not_after.is_some()) {
            this.rotate().await?;
        }
        Ok(this)
    }

    async fn reload(&self) -> anyhow::Result<()> {
        let rows = self.repo.list().await?;
        let now = Utc::now();
        let loaded = rows
            .into_iter()
            .filter(|row| row.not_after.is_none_or(|expiry| expiry > now))
            .map(|row| LoadedKey {
                kid: row.kid,
                private_key_der: decrypt(&self.cipher, &hex::decode(&row.private_key).unwrap_or_default()),
                public_key: hex::decode(&row.public_key).unwrap_or_default(),
                not_after: row.not_after,
            })
            .collect();
        *self.keys.write().unwrap() = loaded;
        Ok(())
    }

    /// Retires the current active key (if any) and generates a new one to
    /// sign with. The old key stays in JWKS until `retire_after_secs` passes.
    pub async fn rotate(&self) -> anyhow::Result<()> {
        let now = Utc::now();
        for mut row in self
            .repo
            .list()
            .await?
            .into_iter()
            .filter(|row| row.not_after.is_none())
        {
            row.not_after = Some(now + chrono::Duration::seconds(self.retire_after_secs));
            self.repo.update(&row).await?;
        }

        let generated = generate_signing_key();
        let row = SigningKeyRow {
            kid: uuid::Uuid::new_v4().to_string(),
            algorithm: "EdDSA".to_string(),
            private_key: hex::encode(encrypt(&self.cipher, &generated.private_key_der)),
            public_key: hex::encode(&generated.public_key),
            created_at: now,
            not_after: None,
        };
        self.repo.create(&row).await?;
        self.reload().await
    }

    /// Every non-retired key, as an RFC 7517 JWK Set.
    pub fn jwks(&self) -> serde_json::Value {
        let keys = self.keys.read().unwrap();
        let entries: Vec<serde_json::Value> = keys
            .iter()
            .map(|key| {
                serde_json::json!({
                    "kty": "OKP",
                    "crv": "Ed25519",
                    "kid": key.kid,
                    "x": URL_SAFE_NO_PAD.encode(&key.public_key),
                })
            })
            .collect();
        serde_json::json!({ "keys": entries })
    }
}

#[async_trait]
impl KeyResolver for SigningKeys {
    async fn resolve(&self, kid: &str) -> Option<DecodingKey> {
        self.keys
            .read()
            .unwrap()
            .iter()
            .find(|key| key.kid == kid)
            .map(|key| decoding_key(&key.public_key))
    }
}

#[async_trait]
impl KeySigner for SigningKeys {
    async fn active(&self) -> Option<(String, EncodingKey)> {
        self.keys
            .read()
            .unwrap()
            .iter()
            .find(|key| key.not_after.is_none())
            .map(|key| (key.kid.clone(), encoding_key(&key.private_key_der)))
    }
}

fn build_cipher() -> anyhow::Result<ChaCha20Poly1305> {
    let material = envmnt::get_or_panic("GATEHOUSE_KEY_ENCRYPTION_KEY");
    let derived = Sha256::digest(material.as_bytes());
    Ok(ChaCha20Poly1305::new(Key::from_slice(&derived)))
}

fn encrypt(cipher: &ChaCha20Poly1305, plaintext: &[u8]) -> Vec<u8> {
    use rand_core::RngCore;
    let mut nonce_bytes = [0u8; 12];
    rand_core::OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .expect("signing key encryption failed");
    let mut out = nonce_bytes.to_vec();
    out.extend(ciphertext);
    out
}

fn decrypt(cipher: &ChaCha20Poly1305, data: &[u8]) -> Vec<u8> {
    let (nonce_bytes, ciphertext) = data.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);
    cipher
        .decrypt(nonce, ciphertext)
        .expect("signing key decryption failed - is GATEHOUSE_KEY_ENCRYPTION_KEY unchanged?")
}
