//! Sealing/opening a secret with XChaCha20-Poly1305 (safe with a random nonce
//! per write). Scope+name are bound in, so a copied/renamed row fails to open.

use base64::Engine;
use chacha20poly1305::aead::common::Generate;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};

/// Bytes in the key. The nonce's length is the cipher's own business - it is
/// generated and checked through `XNonce`, which knows it.
const KEY_BYTES: usize = 32;

#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    #[error(
        "{var} is not set: conveyor cannot read or write secrets. \
         Generate one with `openssl rand -hex 32`."
    )]
    NoKey { var: &'static str },

    #[error("{var} is not {KEY_BYTES} bytes of hex or base64: {reason}")]
    BadKey { var: &'static str, reason: String },

    #[error(
        "a value could not be decrypted. Either the key it was sealed with has \
         changed since it was written, or the row has been altered."
    )]
    CannotOpen,

    #[error("sealing failed")]
    CannotSeal,
}

/// The key this deployment seals secrets with. Not `Debug`/`Clone` on purpose.
pub struct SecretKey {
    cipher: XChaCha20Poly1305,
}

impl SecretKey {
    /// Reads `CONVEYOR_SECRET_KEY` (hex or base64); `Ok(None)` if unset - a
    /// deployment with no secrets shouldn't be forced to have one.
    pub fn from_env() -> Result<Option<Self>, CryptoError> {
        Self::from_env_named("CONVEYOR_SECRET_KEY")
    }

    /// [`from_env`](Self::from_env) reading a differently-named var, so a
    /// different secret kind (git credentials) can rotate its key independently.
    pub fn from_env_named(var: &'static str) -> Result<Option<Self>, CryptoError> {
        let raw = envmnt::get_or(var, "");
        let raw = raw.trim();
        if raw.is_empty() {
            return Ok(None);
        }
        Self::parse(var, raw).map(Some)
    }

    pub fn parse(var: &'static str, raw: &str) -> Result<Self, CryptoError> {
        let bytes = decode_key(var, raw.trim())?;
        if bytes.len() != KEY_BYTES {
            return Err(CryptoError::BadKey {
                var,
                reason: format!("decoded to {} bytes", bytes.len()),
            });
        }

        Ok(Self {
            cipher: XChaCha20Poly1305::new_from_slice(&bytes).map_err(|_| CryptoError::BadKey {
                var,
                reason: "not a usable key".to_string(),
            })?,
        })
    }

    /// Encrypts `value`; `context` (scope+name) must match when opening it again.
    pub fn seal(&self, context: &str, value: &str) -> Result<(Vec<u8>, Vec<u8>), CryptoError> {
        // Random per write - a 24-byte nonce is wide enough to never repeat.
        let nonce = XNonce::try_generate().map_err(|_| CryptoError::CannotSeal)?;

        let ciphertext = self
            .cipher
            .encrypt(
                &nonce,
                Payload {
                    msg: value.as_bytes(),
                    aad: context.as_bytes(),
                },
            )
            .map_err(|_| CryptoError::CannotSeal)?;

        Ok((nonce.to_vec(), ciphertext))
    }

    pub fn open(
        &self,
        context: &str,
        nonce: &[u8],
        ciphertext: &[u8],
    ) -> Result<String, CryptoError> {
        let nonce = <&XNonce>::try_from(nonce).map_err(|_| CryptoError::CannotOpen)?;

        let plaintext = self
            .cipher
            .decrypt(
                nonce,
                Payload {
                    msg: ciphertext,
                    aad: context.as_bytes(),
                },
            )
            .map_err(|_| CryptoError::CannotOpen)?;

        String::from_utf8(plaintext).map_err(|_| CryptoError::CannotOpen)
    }
}

/// Tries hex first, then base64 - both are common paste formats.
fn decode_key(var: &'static str, raw: &str) -> Result<Vec<u8>, CryptoError> {
    if raw.len() == KEY_BYTES * 2
        && let Ok(bytes) = hex::decode(raw)
    {
        return Ok(bytes);
    }

    base64::engine::general_purpose::STANDARD
        .decode(raw)
        .or_else(|_| base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(raw))
        .map_err(|_| CryptoError::BadKey {
            var,
            reason: "not hex, and not base64".to_string(),
        })
}
