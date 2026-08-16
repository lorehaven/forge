//! The realm's one at-rest encryption key, shared by everything gatehouse
//! encrypts with it - signing keys (`keys.rs`) and MFA secrets (`mfa.rs`).
//! Derived (via SHA-256, so any passphrase-shaped string works) from
//! `GATEHOUSE_KEY_ENCRYPTION_KEY` rather than used directly, so the env var
//! itself never has to be exactly 32 bytes.

use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use sha2::{Digest, Sha256};

pub fn realm_cipher() -> anyhow::Result<ChaCha20Poly1305> {
    let material = envmnt::get_or_panic("GATEHOUSE_KEY_ENCRYPTION_KEY");
    let derived: [u8; 32] = Sha256::digest(material.as_bytes()).into();
    Ok(ChaCha20Poly1305::new(&Key::from(derived)))
}

/// `nonce || ChaCha20-Poly1305(plaintext)`, ready to hex-encode for storage -
/// see `keys.rs`'s `SigningKeyRow::private_key` doc comment for why hex
/// rather than raw bytea.
pub fn encrypt(cipher: &ChaCha20Poly1305, plaintext: &[u8]) -> Vec<u8> {
    use rand_core::RngCore;
    let mut nonce_bytes = [0u8; 12];
    rand_core::OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from(nonce_bytes);
    let ciphertext = cipher
        .encrypt(&nonce, plaintext)
        .expect("encryption failed");
    let mut out = nonce_bytes.to_vec();
    out.extend(ciphertext);
    out
}

pub fn decrypt(cipher: &ChaCha20Poly1305, data: &[u8]) -> anyhow::Result<Vec<u8>> {
    if data.len() < 12 {
        anyhow::bail!("ciphertext too short to contain a nonce");
    }
    let (nonce_bytes, ciphertext) = data.split_at(12);
    let nonce_bytes: [u8; 12] = nonce_bytes.try_into().expect("checked above");
    let nonce = Nonce::from(nonce_bytes);
    cipher
        .decrypt(&nonce, ciphertext)
        .map_err(|_| anyhow::anyhow!("decryption failed - is GATEHOUSE_KEY_ENCRYPTION_KEY unchanged?"))
}
