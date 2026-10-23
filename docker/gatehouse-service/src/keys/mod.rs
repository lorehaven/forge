//! Gatehouse's own Ed25519 signing keys - generated at boot, held decrypted
//! in memory so sign/verify never round-trips the database.

use crate::crypto::{decrypt, encrypt, realm_cipher};
use quench_auth::domain::jwt::{KeyResolver, KeySigner};
use quench_auth::domain::signing::{decoding_key, encoding_key, generate_signing_key};
use quench_core::async_trait;
use quench_core::base64::URL_SAFE_NO_PAD;
use quench_core::time::Time;
use quench_crypto::aead::ChaCha20Poly1305;
use quench_crypto::jwt::{DecodingKey, EncodingKey};
use quench_db::prelude::{Crud, Db, Model, Repository};
use quench_serde::{Deserialize, Serialize};
use std::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SigningKeyRow {
    kid: String,
    algorithm: String,
    /// Hex, not raw bytea - `Crud`'s `jsonb_populate_record` round-trip mangles `Vec<u8>`.
    private_key: String,
    public_key: String,
    created_at: Time,
    not_after: Option<Time>,
}

impl Model for SigningKeyRow {
    fn table_name() -> String {
        format!(
            "{}.signing_keys",
            quench_auth::prelude::realm::auth_schema()
        )
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
#[derive(Debug)]
struct LoadedKey {
    kid: String,
    private_key_der: Vec<u8>,
    public_key: Vec<u8>,
    not_after: Option<Time>,
}

pub struct SigningKeys {
    repo: Repository<SigningKeyRow>,
    cipher: ChaCha20Poly1305,
    /// A rotated-out key stays published in JWKS this long, so outstanding tokens keep verifying.
    retire_after_secs: i64,
    keys: RwLock<Vec<LoadedKey>>,
}

impl SigningKeys {
    pub async fn init(db: Db, retire_after_secs: i64) -> quench_core::Result<std::sync::Arc<Self>> {
        let cipher = realm_cipher()?;
        let this = std::sync::Arc::new(Self {
            repo: db.repository::<SigningKeyRow>(),
            cipher,
            retire_after_secs,
            keys: RwLock::new(Vec::new()),
        });
        this.reload().await?;
        if this
            .keys
            .read()
            .unwrap()
            .iter()
            .all(|k| k.not_after.is_some())
        {
            this.rotate().await?;
        }
        Ok(this)
    }

    async fn reload(&self) -> quench_core::Result<()> {
        let rows = self.repo.list().await?;
        let now = Time::now();
        let loaded = rows
            .into_iter()
            .filter(|row| row.not_after.is_none_or(|expiry| expiry > now))
            .map(|row| LoadedKey {
                kid: row.kid,
                private_key_der: decrypt(
                    &self.cipher,
                    &quench_core::hex::decode(&row.private_key).unwrap_or_default(),
                )
                .expect(
                    "signing key decryption failed - is GATEHOUSE_KEY_ENCRYPTION_KEY unchanged?",
                ),
                public_key: quench_core::hex::decode(&row.public_key).unwrap_or_default(),
                not_after: row.not_after,
            })
            .collect();
        *self.keys.write().unwrap() = loaded;
        Ok(())
    }

    /// Retires the active key and generates a new one to sign with.
    pub async fn rotate(&self) -> quench_core::Result<()> {
        let now = Time::now();
        for mut row in self
            .repo
            .list()
            .await?
            .into_iter()
            .filter(|row| row.not_after.is_none())
        {
            row.not_after =
                Some(now + quench_core::time::Duration::seconds(self.retire_after_secs));
            self.repo.update(&row).await?;
        }

        let generated = generate_signing_key();
        let row = SigningKeyRow {
            kid: quench_core::uuid::Uuid::new_v4().to_string(),
            algorithm: "EdDSA".to_string(),
            private_key: quench_core::hex::encode(encrypt(
                &self.cipher,
                &generated.private_key_der,
            )),
            public_key: quench_core::hex::encode(&generated.public_key),
            created_at: now,
            not_after: None,
        };
        self.repo.create(&row).await?;
        self.reload().await
    }

    /// Every non-retired key, as an RFC 7517 JWK Set.
    pub fn jwks(&self) -> quench_serde::Value {
        let keys = self.keys.read().unwrap();
        let entries: Vec<quench_serde::Value> = keys
            .iter()
            .map(|key| {
                quench_serde::json!({
                    "kty": "OKP",
                    "crv": "Ed25519",
                    "kid": key.kid,
                    "x": URL_SAFE_NO_PAD.encode(&key.public_key),
                })
            })
            .collect();
        drop(keys);
        quench_serde::json!({ "keys": entries })
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
            .and_then(|key| {
                encoding_key(&key.private_key_der)
                    .ok()
                    .map(|encoding| (key.kid.clone(), encoding))
            })
    }
}

impl std::fmt::Debug for SigningKeys {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("SigningKeys { .. }")
    }
}
