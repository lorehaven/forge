//! Ed25519 key generation, and the byte shapes gatehouse persists.
//!
//! Kept in one place so `ed25519-dalek`/`pkcs8` stay an implementation detail
//! of this crate: gatehouse's `SigningKeys` (`docker/gatehouse-service/src/keys.rs`)
//! only ever handles opaque `Vec<u8>` and the `jsonwebtoken` key types this
//! module hands back.

use ed25519_dalek::SigningKey;
use ed25519_dalek::pkcs8::EncodePrivateKey;
use jsonwebtoken::{DecodingKey, EncodingKey};

pub struct GeneratedSigningKey {
    /// PKCS8 DER - what `encoding_key` expects back.
    pub private_key_der: Vec<u8>,
    /// Raw 32 bytes - what `decoding_key` and a JWK's `x` expect.
    pub public_key: Vec<u8>,
}

pub fn generate_signing_key() -> GeneratedSigningKey {
    let signing_key = SigningKey::generate(&mut rand_core::OsRng);
    let private_key_der = signing_key
        .to_pkcs8_der()
        .expect("an Ed25519 key always encodes to PKCS8 DER")
        .as_bytes()
        .to_vec();
    let public_key = signing_key.verifying_key().to_bytes().to_vec();
    GeneratedSigningKey {
        private_key_der,
        public_key,
    }
}

pub fn encoding_key(private_key_der: &[u8]) -> EncodingKey {
    EncodingKey::from_ed_der(private_key_der)
}

pub fn decoding_key(public_key: &[u8]) -> DecodingKey {
    DecodingKey::from_ed_der(public_key)
}
