//! TOTP-based MFA. Lives here, not `quench-auth`, since only gatehouse's own
//! login ever challenges for a code - the only code that decrypts the secret.

use crate::crypto::{decrypt, encrypt, realm_cipher};
use quench_crypto::hmac::{self, Algorithm as Hmac};
use quench_crypto::totp::{self, Totp};

const ISSUER: &str = "Forge";

/// Password-verified pending-MFA token lifetime - short enough a stolen one
/// is useless, long enough not to time out fumbling for an authenticator app.
const PENDING_TTL_SECS: i64 = 120;

/// Fresh random secret, base32-encoded for manual-entry display.
pub fn generate_secret() -> quench_core::Result<String> {
    Ok(Totp::new(totp::generate_secret())
        .map_err(|err| quench_core::err!("failed to build TOTP: {err}"))?
        .secret_base32())
}

/// Encrypted at rest under the same key `keys.rs` uses for signing keys.
pub fn encrypt_secret(secret: &str) -> quench_core::Result<String> {
    let cipher = realm_cipher()?;
    Ok(quench_core::hex::encode(encrypt(
        &cipher,
        secret.as_bytes(),
    )))
}

pub fn decrypt_secret(encrypted_hex: &str) -> quench_core::Result<String> {
    let cipher = realm_cipher()?;
    let bytes = quench_core::hex::decode(encrypted_hex)?;
    let plaintext = decrypt(&cipher, &bytes)?;
    Ok(String::from_utf8(plaintext)?)
}

/// `otpauth://totp/...` for the enrollment QR - shown once, never rebuilt.
pub fn provisioning_uri(secret: &str, username: &str) -> quench_core::Result<String> {
    let totp =
        Totp::from_base32(secret).map_err(|err| quench_core::err!("invalid secret: {err}"))?;
    Ok(totp.url(ISSUER, username))
}

/// The provisioning URI as an inline SVG data URI, so the page needs no image route.
pub fn provisioning_qr_data_uri(uri: &str) -> quench_core::Result<String> {
    let svg = quench_qr::QrCode::new(uri.as_bytes())?.to_svg(200);
    let encoded = quench_core::base64::STANDARD.encode(svg);
    Ok(format!("data:image/svg+xml;base64,{encoded}"))
}

/// Whether `code` is a valid current TOTP code for `secret` (base32).
#[must_use]
pub fn verify_code(secret: &str, code: &str) -> bool {
    Totp::from_base32(secret).is_ok_and(|totp| totp.check_now(code))
}

fn pending_key() -> Vec<u8> {
    let material = std::env::var("GATEHOUSE_KEY_ENCRYPTION_KEY")
        .expect(concat!("GATEHOUSE_KEY_ENCRYPTION_KEY", " must be set"));
    quench_crypto::hash::sha256(material.as_bytes()).to_vec()
}

/// Signs `username` plus an expiry, carried as a hidden form field instead
/// of a server-side session store.
#[must_use]
pub fn sign_pending(username: &str) -> String {
    let expires_at = quench_core::time::Time::now().timestamp() + PENDING_TTL_SECS;
    let payload = format!("{username}:{expires_at}");
    let key = pending_key();
    let signature = quench_core::hex::encode(hmac::sign(Hmac::Sha256, &key, payload.as_bytes()));
    format!("{payload}:{signature}")
}

/// The username a pending token was signed for, if valid and unexpired.
#[must_use]
pub fn verify_pending(token: &str) -> Option<String> {
    let (payload, signature) = token.rsplit_once(':')?;
    let (username, expires_at) = payload.rsplit_once(':')?;

    let key = pending_key();
    let tag = quench_core::hex::decode(signature).ok()?;
    if !hmac::verify(Hmac::Sha256, &key, payload.as_bytes(), &tag) {
        return None;
    }

    let expires_at: i64 = expires_at.parse().ok()?;
    if quench_core::time::Time::now().timestamp() > expires_at {
        return None;
    }

    Some(username.to_string())
}
