use gatehouse_service::mfa::{
    decrypt_secret, encrypt_secret, generate_secret, provisioning_uri, sign_pending, verify_code,
    verify_pending,
};
use gatehouse_service::test_support::TEST_KEY_MATERIAL;
use hmac::{Hmac, KeyInit, Mac};
use sha2::{Digest, Sha256};
use totp_rs::{Algorithm, Builder, Secret, Totp};

type HmacSha256 = Hmac<Sha256>;

fn totp(secret: Secret, username: &str) -> Totp {
    Builder::new()
        .with_algorithm(Algorithm::SHA1)
        .with_digits(6)
        .with_skew(1)
        .with_step_duration(30)
        .with_secret(secret)
        .with_account_name(username.to_string())
        .with_issuer(Some("Forge"))
        .build()
        .expect("build totp")
}

/// `realm_cipher`/`encrypt_secret`/`decrypt_secret`/`sign_pending`/
/// `verify_pending` all read `GATEHOUSE_KEY_ENCRYPTION_KEY` via
/// `envmnt::get_or_panic`, so every test that touches them needs it set -
/// cargo test runs a crate's tests in one process, so this has to be
/// idempotent across tests rather than set-once.
fn with_key<T>(run: impl FnOnce() -> T) -> T {
    envmnt::set("GATEHOUSE_KEY_ENCRYPTION_KEY", TEST_KEY_MATERIAL);
    run()
}

#[test]
fn a_generated_secret_verifies_its_own_current_code() {
    let secret = generate_secret().unwrap();
    let parsed = Secret::try_from_base32(&secret).unwrap();
    let code = totp(parsed, "someone").generate_current();

    assert!(verify_code(&secret, &code.to_string()));
}

#[test]
fn a_code_for_a_different_secret_does_not_verify() {
    let secret = generate_secret().unwrap();
    let other_secret = generate_secret().unwrap();
    let other_parsed = Secret::try_from_base32(&other_secret).unwrap();
    let code = totp(other_parsed, "someone").generate_current();

    assert!(!verify_code(&secret, &code.to_string()));
}

#[test]
fn provisioning_uri_carries_the_issuer_and_account_name() {
    let secret = generate_secret().unwrap();
    let uri = provisioning_uri(&secret, "someone").unwrap();

    assert!(uri.starts_with("otpauth://totp/"));
    assert!(uri.contains("Forge"));
    assert!(uri.contains("someone"));
}

#[test]
fn a_secret_round_trips_through_encryption_at_rest() {
    with_key(|| {
        let secret = generate_secret().unwrap();
        let encrypted = encrypt_secret(&secret).unwrap();
        assert_ne!(encrypted, secret);
        assert_eq!(decrypt_secret(&encrypted).unwrap(), secret);
    });
}

#[test]
fn a_pending_token_verifies_back_to_the_username_it_was_signed_for() {
    with_key(|| {
        let token = sign_pending("someone").unwrap();
        assert_eq!(verify_pending(&token).as_deref(), Some("someone"));
    });
}

#[test]
fn a_tampered_pending_token_does_not_verify() {
    with_key(|| {
        let token = sign_pending("someone").unwrap();
        let tampered = token.replace("someone", "someone-else");
        assert_eq!(verify_pending(&tampered), None);
    });
}

#[test]
fn an_expired_pending_token_does_not_verify() {
    with_key(|| {
        let expired_at = chrono::Utc::now().timestamp() - 1;
        let payload = format!("someone:{expired_at}");
        // `pending_key()` is private, so this mirrors it exactly: SHA-256 of
        // the shared key-encryption material, same as `mfa::pending_key`.
        let key = Sha256::digest(TEST_KEY_MATERIAL.as_bytes()).to_vec();
        let mut mac = HmacSha256::new_from_slice(&key).unwrap();
        mac.update(payload.as_bytes());
        let signature = hex::encode(mac.finalize().into_bytes());
        let token = format!("{payload}:{signature}");

        assert_eq!(verify_pending(&token), None);
    });
}
