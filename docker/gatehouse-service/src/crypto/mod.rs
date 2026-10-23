//! The realm's one at-rest encryption key (signing keys, MFA secrets),
//! SHA-256-derived from `GATEHOUSE_KEY_ENCRYPTION_KEY` so it need not be 32 bytes.

use quench_crypto::aead::ChaCha20Poly1305;

pub fn realm_cipher() -> quench_core::Result<ChaCha20Poly1305> {
    let material = std::env::var("GATEHOUSE_KEY_ENCRYPTION_KEY")
        .expect(concat!("GATEHOUSE_KEY_ENCRYPTION_KEY", " must be set"));
    let derived: [u8; 32] = quench_crypto::hash::sha256(material.as_bytes());
    Ok(ChaCha20Poly1305::new(derived))
}

/// `nonce || ChaCha20-Poly1305(plaintext)`, ready to hex-encode for storage.
#[must_use]
pub fn encrypt(cipher: &ChaCha20Poly1305, plaintext: &[u8]) -> Vec<u8> {
    cipher.seal(&[], plaintext)
}

pub fn decrypt(cipher: &ChaCha20Poly1305, data: &[u8]) -> quench_core::Result<Vec<u8>> {
    if data.len() < quench_crypto::aead::NONCE_LEN {
        quench_core::bail!("ciphertext too short to contain a nonce");
    }
    cipher.open(&[], data).map_err(|_| {
        quench_core::err!("decryption failed - is GATEHOUSE_KEY_ENCRYPTION_KEY unchanged?")
    })
}
