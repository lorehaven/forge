use chacha20poly1305::{ChaCha20Poly1305, Key, KeyInit};
use gatehouse_service::crypto::{decrypt, encrypt, realm_cipher};
use gatehouse_service::test_support::TEST_KEY_MATERIAL;
use sha2::{Digest, Sha256};

fn test_cipher() -> ChaCha20Poly1305 {
    envmnt::set("GATEHOUSE_KEY_ENCRYPTION_KEY", TEST_KEY_MATERIAL);
    realm_cipher().expect("cipher")
}

/// A cipher derived from different material than `test_cipher`'s, without
/// touching the shared env var at all - so "decrypting with the wrong
/// key fails" can be tested without racing every other module's tests
/// that depend on `GATEHOUSE_KEY_ENCRYPTION_KEY` staying `TEST_KEY_MATERIAL`.
fn other_cipher() -> ChaCha20Poly1305 {
    let derived: [u8; 32] = Sha256::digest(b"a-completely-different-key-material").into();
    ChaCha20Poly1305::new(&Key::from(derived))
}

#[test]
fn realm_cipher_derives_a_usable_key_from_any_length_passphrase() {
    let cipher = test_cipher();
    let plaintext = b"round trips";
    let ciphertext = encrypt(&cipher, plaintext);
    assert_eq!(decrypt(&cipher, &ciphertext).expect("decrypt"), plaintext);
}

#[test]
fn encrypt_then_decrypt_round_trips_arbitrary_bytes() {
    let cipher = test_cipher();
    let plaintext = b"the quick brown fox jumps over the lazy dog";
    let ciphertext = encrypt(&cipher, plaintext);

    assert_ne!(ciphertext.as_slice(), plaintext.as_slice());
    assert_eq!(decrypt(&cipher, &ciphertext).expect("decrypt"), plaintext);
}

#[test]
fn encrypt_uses_a_fresh_nonce_each_time() {
    let cipher = test_cipher();
    let a = encrypt(&cipher, b"same plaintext");
    let b = encrypt(&cipher, b"same plaintext");
    assert_ne!(a, b, "ciphertexts must differ due to random nonces");
}

#[test]
fn decrypt_rejects_data_too_short_to_hold_a_nonce() {
    let cipher = test_cipher();
    let error = decrypt(&cipher, &[1, 2, 3]).unwrap_err();
    assert!(error.to_string().contains("too short"));
}

#[test]
fn decrypt_rejects_tampered_ciphertext() {
    let cipher = test_cipher();
    let mut ciphertext = encrypt(&cipher, b"tamper with me");
    let last = ciphertext.len() - 1;
    ciphertext[last] ^= 0xFF;

    let error = decrypt(&cipher, &ciphertext).unwrap_err();
    assert!(error.to_string().contains("decryption failed"));
}

#[test]
fn decrypt_rejects_ciphertext_from_a_different_key() {
    let cipher_a = test_cipher();
    let ciphertext = encrypt(&cipher_a, b"secret");

    let cipher_b = other_cipher();
    assert!(decrypt(&cipher_b, &ciphertext).is_err());
}
