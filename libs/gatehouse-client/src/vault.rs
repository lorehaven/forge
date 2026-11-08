//! A session file that is unreadable without the device's key.
//!
//! On Android the host (Kotlin) keeps a random data key wrapped by an Android Keystore key, which never leaves the
//! secure hardware/system service, and hands the unwrapped data key to the program in [`KEY_VAR`] (64 hex digits).
//! Whatever the program writes with [`seal`] is then useless to anything that only has the app's files (a backup, a
//! copied data directory). Where no key is given (a desktop) text is stored as it is, as before, and a plain file
//! left by an earlier version is still read.

use quench_crypto::aead::XChaCha20Poly1305;

/// The environment variable that holds the data key.
pub const KEY_VAR: &str = "FORGE_SESSION_KEY";

/// Marks a sealed value; anything else is plain text.
const PREFIX: &str = "enc1:";

/// What binds a sealed value to this use: it opens nowhere else.
const AAD: &[u8] = b"forge session";

fn cipher(key_hex: Option<&str>) -> Option<XChaCha20Poly1305> {
    let key: [u8; 32] = quench_core::hex::decode(key_hex?.trim())
        .ok()?
        .try_into()
        .ok()?;
    Some(XChaCha20Poly1305::new(key))
}

fn host_key() -> Option<String> {
    std::env::var(KEY_VAR).ok()
}

/// `text` as it should be written: sealed when the host gave a key, else as it is.
#[must_use]
pub fn seal(text: &str) -> String {
    seal_with(host_key().as_deref(), text)
}

/// What [`seal`] was given. `None` for a sealed value that cannot be opened (the key is gone or is another one).
#[must_use]
pub fn open(stored: &str) -> Option<String> {
    open_with(host_key().as_deref(), stored)
}

/// [`seal`] with the key (64 hex digits) given instead of read from the environment.
#[must_use]
pub fn seal_with(key_hex: Option<&str>, text: &str) -> String {
    cipher(key_hex).map_or_else(
        || text.to_string(),
        |c| {
            format!(
                "{PREFIX}{}",
                quench_core::hex::encode(c.seal(AAD, text.as_bytes()))
            )
        },
    )
}

/// [`open`] with the key (64 hex digits) given instead of read from the environment.
#[must_use]
pub fn open_with(key_hex: Option<&str>, stored: &str) -> Option<String> {
    let stored = stored.trim();
    let Some(sealed) = stored.strip_prefix(PREFIX) else {
        return Some(stored.to_string());
    };
    let bytes = quench_core::hex::decode(sealed).ok()?;
    let plain = cipher(key_hex)?.open(AAD, &bytes).ok()?;
    String::from_utf8(plain).ok()
}
