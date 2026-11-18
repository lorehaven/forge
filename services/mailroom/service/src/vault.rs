//! Sealing the passwords of other people's mailboxes.
//!
//! A mailbox password is a credential for somebody's whole mail life. It is sealed with XChaCha20-Poly1305 under a key
//! derived (HKDF-SHA256) from `MAILROOM_VAULT_KEY`; the account's id and owner are the associated data, so a sealed value
//! copied from one row to another does not open. The database, its backups and any API response see only the sealed text.

use quench_crypto::aead::XChaCha20Poly1305;
use quench_crypto::hkdf;
use std::fmt::Write as _;

const PREFIX: &str = "v1.";
const SALT: &[u8] = b"mailroom-vault-v1";
/// Shortest key material accepted.
pub const MIN_KEY_LENGTH: usize = 32;

/// Why a key or a sealed value was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VaultError {
    /// The key material is shorter than [`MIN_KEY_LENGTH`].
    WeakKey,
    /// The sealed text is malformed, or was sealed under another key or for another account.
    Unreadable,
}

impl std::fmt::Display for VaultError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::WeakKey => "the vault key is too short (32 characters at least)",
            Self::Unreadable => "a stored secret cannot be opened",
        })
    }
}

impl std::error::Error for VaultError {}

/// Seals and opens secrets.
#[derive(Clone)]
pub struct Vault {
    current: XChaCha20Poly1305,
    previous: Option<XChaCha20Poly1305>,
}

impl std::fmt::Debug for Vault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Vault { .. }")
    }
}

fn derive(material: &str) -> Result<XChaCha20Poly1305, VaultError> {
    if material.len() < MIN_KEY_LENGTH {
        return Err(VaultError::WeakKey);
    }
    let key =
        hkdf::sha256(material.as_bytes(), SALT, b"seal", 32).map_err(|_| VaultError::WeakKey)?;
    let key: [u8; 32] = key.try_into().map_err(|_| VaultError::WeakKey)?;
    Ok(XChaCha20Poly1305::new(key))
}

impl Vault {
    /// A vault under `key`, which opens values sealed under `previous` too (and seals only under `key`).
    ///
    /// # Errors
    ///
    /// [`VaultError::WeakKey`] when a key is too short.
    pub fn new(key: &str, previous: Option<&str>) -> Result<Self, VaultError> {
        Ok(Self {
            current: derive(key)?,
            previous: previous.map(derive).transpose()?,
        })
    }

    /// A vault under a key made up for this run: what it seals does not open after a restart.
    #[must_use]
    pub fn ephemeral() -> Self {
        let bytes: [u8; 32] = quench_core::random::bytes();
        let key = bytes.iter().fold(String::new(), |mut acc, b| {
            let _ = write!(acc, "{b:02x}");
            acc
        });
        Self::new(&key, None).expect("64 hex characters are long enough")
    }

    /// `v1.` and base64 of nonce, ciphertext and tag.
    #[must_use]
    pub fn seal(&self, context: &str, secret: &str) -> String {
        let blob = self.current.seal(context.as_bytes(), secret.as_bytes());
        format!(
            "{PREFIX}{}",
            quench_core::base64::STANDARD_NO_PAD.encode(blob)
        )
    }

    /// The secret in `sealed`, which was sealed for `context`. The second value says whether it was opened with the
    /// previous key (so the caller may seal it again under the current one).
    ///
    /// # Errors
    ///
    /// [`VaultError::Unreadable`].
    pub fn open(&self, context: &str, sealed: &str) -> Result<(String, bool), VaultError> {
        let body = sealed.strip_prefix(PREFIX).ok_or(VaultError::Unreadable)?;
        let blob = quench_core::base64::STANDARD_NO_PAD
            .decode(body)
            .map_err(|_| VaultError::Unreadable)?;
        for (key, old) in [(Some(&self.current), false), (self.previous.as_ref(), true)] {
            let Some(key) = key else { continue };
            if let Ok(plain) = key.open(context.as_bytes(), &blob) {
                let text = String::from_utf8(plain).map_err(|_| VaultError::Unreadable)?;
                return Ok((text, old));
            }
        }
        Err(VaultError::Unreadable)
    }
}

/// The associated data for an account's password.
#[must_use]
pub fn account_context(account_id: &str, owner: &str) -> String {
    format!("account:{account_id}:{owner}")
}

/// The associated data for the estate's transport password.
pub const TRANSPORT_CONTEXT: &str = "transport:main";
