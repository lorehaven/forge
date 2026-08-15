//! Sealing and opening a secret.
//!
//! XChaCha20-Poly1305, whose 24-byte nonce is large enough that generating one
//! at random per write is safe. The 12-byte nonces of the AES-GCM family are
//! not: reusing one leaks the plaintext, and "count carefully across every
//! replica" is not a property a service should have to maintain.
//!
//! Each value is bound to where it lives. The scope and name go in as
//! associated data, so a row copied from one repository to another - or renamed
//! in place by somebody with write access to the database but not the key -
//! fails to open rather than quietly granting a secret to a repository that was
//! never given one.

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

/// The key this deployment seals secrets with.
///
/// Deliberately not `Debug` or `Clone`: a key that can be printed ends up in a
/// log, and one that can be cloned ends up in more places than it needs to be.
pub struct SecretKey {
    cipher: XChaCha20Poly1305,
}

impl SecretKey {
    /// Reads `CONVEYOR_SECRET_KEY`, accepting hex or base64.
    ///
    /// `Ok(None)` when it is unset - a deployment with no secrets does not need
    /// one, and refusing to start would make the whole service depend on a
    /// feature it may never use. Reading or writing a secret without it is an
    /// error at that point, which is where it can be reported usefully.
    pub fn from_env() -> Result<Option<Self>, CryptoError> {
        Self::from_env_named("CONVEYOR_SECRET_KEY")
    }

    /// [`from_env`](Self::from_env), reading a differently-named variable.
    ///
    /// Lets a value sealed for a different purpose - git credentials, say -
    /// use its own key and be rotated or compromised independently, without a
    /// second copy of the cipher wiring below.
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

    /// Encrypts `value`, returning the nonce and the ciphertext.
    ///
    /// `context` is the scope and name this value belongs to; the same string
    /// has to be supplied to open it again.
    pub fn seal(&self, context: &str, value: &str) -> Result<(Vec<u8>, Vec<u8>), CryptoError> {
        // From the operating system, per write. A 24-byte nonce is wide enough
        // that random generation will not repeat one before the heat death of
        // the deployment, which is what makes this safe without a counter.
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

/// Hex first, then base64. Both are things people paste out of a password
/// manager, and telling them apart by trying is kinder than making them say
/// which they have.
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
