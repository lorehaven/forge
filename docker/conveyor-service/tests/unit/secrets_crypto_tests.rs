//! Unit tests for `secrets/crypto.rs`.

use conveyor_service::secrets::SecretKey;
use conveyor_service::secrets::crypto::CryptoError;

const HEX_KEY: &str = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";

fn key() -> SecretKey {
    SecretKey::parse(HEX_KEY).expect("a 32-byte hex key")
}

#[test]
fn a_value_survives_a_round_trip() {
    let key = key();
    let (nonce, ciphertext) = key
        .seal("repo:1:TOKEN", "hunter2-and-then-some")
        .expect("seal");

    assert_eq!(
        key.open("repo:1:TOKEN", &nonce, &ciphertext).expect("open"),
        "hunter2-and-then-some"
    );
}

#[test]
fn the_ciphertext_does_not_contain_the_plaintext() {
    let (_, ciphertext) = key()
        .seal("global:TOKEN", "a-recognisable-value")
        .expect("seal");
    let rendered = String::from_utf8_lossy(&ciphertext);
    assert!(!rendered.contains("a-recognisable-value"));
}

#[test]
fn sealing_the_same_value_twice_gives_different_ciphertext() {
    // A fresh nonce per write. Without one, two repositories with the same
    // token would be visibly identical in the table.
    let key = key();
    let (first_nonce, first) = key.seal("global:TOKEN", "same value").expect("seal");
    let (second_nonce, second) = key.seal("global:TOKEN", "same value").expect("seal");

    assert_ne!(first_nonce, second_nonce);
    assert_ne!(first, second);
}

#[test]
fn a_value_sealed_for_one_scope_will_not_open_in_another() {
    // The point of binding scope and name into the associated data: a row
    // copied from one repository to another by somebody with write access to
    // the database, but not the key, fails to open rather than quietly granting
    // a secret to a repository that was never given one.
    let key = key();
    let (nonce, ciphertext) = key.seal("repo:alpha:TOKEN", "alpha's token").expect("seal");

    assert!(key.open("repo:beta:TOKEN", &nonce, &ciphertext).is_err());
    assert!(key.open("global:TOKEN", &nonce, &ciphertext).is_err());
}

#[test]
fn a_value_will_not_open_under_a_different_name() {
    let key = key();
    let (nonce, ciphertext) = key.seal("repo:alpha:TOKEN", "a token").expect("seal");
    assert!(key.open("repo:alpha:OTHER", &nonce, &ciphertext).is_err());
}

#[test]
fn another_key_cannot_open_it() {
    let (nonce, ciphertext) = key().seal("global:TOKEN", "a token").expect("seal");
    let other = SecretKey::parse(&"ab".repeat(32)).expect("another key");

    assert!(matches!(
        other.open("global:TOKEN", &nonce, &ciphertext),
        Err(CryptoError::CannotOpen)
    ));
}

#[test]
fn a_tampered_ciphertext_is_rejected() {
    // Poly1305 is what makes this a detection rather than a garbled plaintext.
    let key = key();
    let (nonce, mut ciphertext) = key.seal("global:TOKEN", "a token").expect("seal");
    ciphertext[0] ^= 0xff;

    assert!(key.open("global:TOKEN", &nonce, &ciphertext).is_err());
}

#[test]
fn a_tampered_nonce_is_rejected() {
    let key = key();
    let (mut nonce, ciphertext) = key.seal("global:TOKEN", "a token").expect("seal");
    nonce[0] ^= 0xff;

    assert!(key.open("global:TOKEN", &nonce, &ciphertext).is_err());
}

#[test]
fn a_nonce_of_the_wrong_length_is_rejected_rather_than_panicking() {
    let key = key();
    let (_, ciphertext) = key.seal("global:TOKEN", "a token").expect("seal");

    assert!(key.open("global:TOKEN", &[], &ciphertext).is_err());
    assert!(key.open("global:TOKEN", &[0u8; 12], &ciphertext).is_err());
    assert!(key.open("global:TOKEN", &[0u8; 64], &ciphertext).is_err());
}

#[test]
fn an_empty_value_round_trips() {
    // The store refuses one, but the cipher should not be the thing that
    // decides that.
    let key = key();
    let (nonce, ciphertext) = key.seal("global:TOKEN", "").expect("seal");
    assert_eq!(key.open("global:TOKEN", &nonce, &ciphertext).unwrap(), "");
}

#[test]
fn a_value_with_awkward_bytes_round_trips() {
    let key = key();
    let value = "line\nbreak\ttab \u{1F511} ünïcödé";
    let (nonce, ciphertext) = key.seal("global:TOKEN", value).expect("seal");
    assert_eq!(
        key.open("global:TOKEN", &nonce, &ciphertext).unwrap(),
        value
    );
}

// ---------------------------------------------------------------------------
// Keys
// ---------------------------------------------------------------------------

#[test]
fn a_hex_key_is_accepted() {
    assert!(SecretKey::parse(HEX_KEY).is_ok());
    assert!(SecretKey::parse(&format!("  {HEX_KEY}  ")).is_ok());
}

#[test]
fn a_base64_key_is_accepted() {
    // Both are things people paste out of a password manager.
    use base64::Engine;
    let bytes = hex::decode(HEX_KEY).expect("hex");
    let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
    assert!(SecretKey::parse(&encoded).is_ok());
}

#[test]
fn a_key_of_the_wrong_length_is_refused() {
    assert!(matches!(
        SecretKey::parse("00112233"),
        Err(CryptoError::BadKey { .. })
    ));
    assert!(SecretKey::parse(&"ab".repeat(64)).is_err());
}

#[test]
fn a_key_that_is_neither_hex_nor_base64_is_refused() {
    assert!(matches!(
        SecretKey::parse("this is not a key at all!!!"),
        Err(CryptoError::BadKey { .. })
    ));
}

#[test]
fn hex_and_base64_of_the_same_bytes_produce_the_same_key() {
    use base64::Engine;
    let bytes = hex::decode(HEX_KEY).expect("hex");
    let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);

    let from_hex = SecretKey::parse(HEX_KEY).expect("hex key");
    let from_base64 = SecretKey::parse(&encoded).expect("base64 key");

    let (nonce, ciphertext) = from_hex.seal("global:TOKEN", "value").expect("seal");
    assert_eq!(
        from_base64
            .open("global:TOKEN", &nonce, &ciphertext)
            .unwrap(),
        "value"
    );
}
