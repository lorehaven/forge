//! TOTP-based multi-factor authentication.
//!
//! Lives here rather than in `quench-auth` because verifying a code is
//! interactive - only gatehouse's own login page ever challenges for one, no
//! relying party's machine-to-machine path does - the same reasoning
//! `keys.rs`'s signing-key crypto stays out of the shared library too.
//! `User.mfa_secret` is an opaque, already-encrypted string as far as
//! `quench-auth` is concerned; this module is the only code that ever
//! decrypts or checks it.

use crate::crypto::{decrypt, encrypt, realm_cipher};
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use totp_rs::{Algorithm, Builder, Secret, Totp};

const ISSUER: &str = "Forge";

/// How long a "you already gave the right password, now give a code" token
/// stays good for. Short enough that a stolen intermediate value is useless
/// by the time anyone could do anything with it; long enough that fumbling
/// an authenticator app open doesn't time out.
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

/// A fresh random secret, base32-encoded for display (an authenticator app's
/// "enter this code manually" fallback when it can't scan a QR).
pub fn generate_secret() -> anyhow::Result<String> {
    Ok(Secret::generate().to_base32())
}

/// Encrypted at rest under the same key `keys.rs` uses for signing keys - see
/// `crypto::realm_cipher`. `secret` is the base32 string from
/// `generate_secret`.
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

/// `otpauth://totp/...` for a QR code or manual entry, shown once at
/// enrollment - never reconstructable afterward without the plaintext
/// secret, which is why enrollment is a one-shot "here it is, now prove you
/// saved it" flow rather than something revisitable.
pub fn provisioning_uri(secret: &str, username: &str) -> anyhow::Result<String> {
    let secret = Secret::try_from_base32(secret)
        .map_err(|err| anyhow::anyhow!("invalid secret: {err:?}"))?;
    totp(secret, username)?
        .to_url()
        .map_err(|err| anyhow::anyhow!("failed to build provisioning URI: {err}"))
}

/// Whether `code` is a valid current TOTP code for `secret` (base32, as
/// returned by `generate_secret`/stored decrypted).
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

/// Signs `username` plus an expiry into an opaque token, carried through the
/// login → MFA-challenge form as a hidden field rather than a second
/// server-side session store - see the module doc comment on why a stolen
/// one is only useful for two minutes.
pub fn sign_pending(username: &str) -> anyhow::Result<String> {
    let expires_at = chrono::Utc::now().timestamp() + PENDING_TTL_SECS;
    let payload = format!("{username}:{expires_at}");
    let key = pending_key()?;
    let mut mac = HmacSha256::new_from_slice(&key).expect("any key length");
    mac.update(payload.as_bytes());
    let signature = hex::encode(mac.finalize().into_bytes());
    Ok(format!("{payload}:{signature}"))
}

/// The username a pending token was signed for, if the signature checks out
/// and it has not expired.
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
