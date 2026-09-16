//! TOTP-based MFA. Lives here, not `quench-auth`, since only gatehouse's own
//! login ever challenges for a code - the only code that decrypts the secret.

use crate::crypto::{decrypt, encrypt, realm_cipher};
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use totp_rs::{Algorithm, Builder, Secret, Totp};

const ISSUER: &str = "Forge";

/// Password-verified pending-MFA token lifetime - short enough a stolen one
/// is useless, long enough not to time out fumbling for an authenticator app.
const PENDING_TTL_SECS: i64 = 120;

fn totp(secret: Secret, username: &str) -> anyhow::Result<Totp> {
    Builder::new()
        .with_algorithm(Algorithm::SHA1)
        .with_digits(6)
        .with_skew(1)
        .with_step_duration(30)
        .with_secret(secret)
        .with_account_name(username.to_string())
        .with_issuer(Some(ISSUER))
        .build()
        .map_err(|err| anyhow::anyhow!("failed to build TOTP: {err}"))
}

/// Fresh random secret, base32-encoded for manual-entry display.
pub fn generate_secret() -> anyhow::Result<String> {
    Ok(Secret::generate().to_base32())
}

/// Encrypted at rest under the same key `keys.rs` uses for signing keys.
pub fn encrypt_secret(secret: &str) -> anyhow::Result<String> {
    let cipher = realm_cipher()?;
    Ok(hex::encode(encrypt(&cipher, secret.as_bytes())))
}

pub fn decrypt_secret(encrypted_hex: &str) -> anyhow::Result<String> {
    let cipher = realm_cipher()?;
    let bytes = hex::decode(encrypted_hex)?;
    let plaintext = decrypt(&cipher, &bytes)?;
    Ok(String::from_utf8(plaintext)?)
}

/// `otpauth://totp/...` for the enrollment QR - shown once, never rebuilt.
pub fn provisioning_uri(secret: &str, username: &str) -> anyhow::Result<String> {
    let secret = Secret::try_from_base32(secret)
        .map_err(|err| anyhow::anyhow!("invalid secret: {err:?}"))?;
    totp(secret, username)?
        .to_url()
        .map_err(|err| anyhow::anyhow!("failed to build provisioning URI: {err}"))
}

/// Whether `code` is a valid current TOTP code for `secret` (base32).
pub fn verify_code(secret: &str, code: &str) -> bool {
    let Ok(secret) = Secret::try_from_base32(secret) else {
        return false;
    };
    let Ok(totp) = totp(secret, "") else {
        return false;
    };
    totp.check_current(code).is_some()
}

type HmacSha256 = Hmac<Sha256>;

fn pending_key() -> anyhow::Result<Vec<u8>> {
    use sha2::Digest;
    let material = envmnt::get_or_panic("GATEHOUSE_KEY_ENCRYPTION_KEY");
    Ok(Sha256::digest(material.as_bytes()).to_vec())
}

/// Signs `username` plus an expiry, carried as a hidden form field instead
/// of a server-side session store.
pub fn sign_pending(username: &str) -> anyhow::Result<String> {
    let expires_at = chrono::Utc::now().timestamp() + PENDING_TTL_SECS;
    let payload = format!("{username}:{expires_at}");
    let key = pending_key()?;
    let mut mac = HmacSha256::new_from_slice(&key).expect("any key length");
    mac.update(payload.as_bytes());
    let signature = hex::encode(mac.finalize().into_bytes());
    Ok(format!("{payload}:{signature}"))
}

/// The username a pending token was signed for, if valid and unexpired.
pub fn verify_pending(token: &str) -> Option<String> {
    let (payload, signature) = token.rsplit_once(':')?;
    let (username, expires_at) = payload.rsplit_once(':')?;

    let key = pending_key().ok()?;
    let mut mac = HmacSha256::new_from_slice(&key).ok()?;
    mac.update(payload.as_bytes());
    let expected = hex::encode(mac.finalize().into_bytes());
    if expected != signature {
        return None;
    }

    let expires_at: i64 = expires_at.parse().ok()?;
    if chrono::Utc::now().timestamp() > expires_at {
        return None;
    }

    Some(username.to_string())
}
