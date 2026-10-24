use gatehouse_service::mfa::{
    decrypt_secret, encrypt_secret, generate_secret, provisioning_uri, sign_pending, verify_code,
    verify_pending,
};
use gatehouse_service::test_support::TEST_KEY_MATERIAL;
use quench_crypto::totp::Totp;

/// `realm_cipher`/`encrypt_secret`/`decrypt_secret`/`sign_pending`/
/// `verify_pending` all read `GATEHOUSE_KEY_ENCRYPTION_KEY` via
/// `quench_core::env::get_or_panic`, so every test that touches them needs it set -
/// cargo test runs a crate's tests in one process, so this has to be
/// idempotent across tests rather than set-once.
fn with_key<T>(run: impl FnOnce() -> T) -> T {
    quench_core::env::set("GATEHOUSE_KEY_ENCRYPTION_KEY", TEST_KEY_MATERIAL);
    run()
}

#[quench_test::test]
fn a_generated_secret_verifies_its_own_current_code() {
    let secret = generate_secret().unwrap();
    let code = Totp::from_base32(&secret).unwrap().code_now();

    assert!(verify_code(&secret, &code));
}

#[quench_test::test]
fn a_code_for_a_different_secret_does_not_verify() {
    let secret = generate_secret().unwrap();
    let other_secret = generate_secret().unwrap();
    let code = Totp::from_base32(&other_secret).unwrap().code_now();

    assert!(!verify_code(&secret, &code));
}

#[quench_test::test]
fn provisioning_uri_carries_the_issuer_and_account_name() {
    let secret = generate_secret().unwrap();
    let uri = provisioning_uri(&secret, "someone").unwrap();

    assert!(uri.starts_with("otpauth://totp/"));
    assert!(uri.contains("Forge"));
    assert!(uri.contains("someone"));
}

#[quench_test::test]
fn a_secret_round_trips_through_encryption_at_rest() {
    with_key(|| {
        let secret = generate_secret().unwrap();
        let encrypted = encrypt_secret(&secret).unwrap();
        assert_ne!(encrypted, secret);
        assert_eq!(decrypt_secret(&encrypted).unwrap(), secret);
    });
}

#[quench_test::test]
fn a_pending_token_verifies_back_to_the_username_it_was_signed_for() {
    with_key(|| {
        let token = sign_pending("someone");
        assert_eq!(verify_pending(&token).as_deref(), Some("someone"));
    });
}

#[quench_test::test]
fn a_tampered_pending_token_does_not_verify() {
    with_key(|| {
        let token = sign_pending("someone");
        let tampered = token.replace("someone", "someone-else");
        assert_eq!(verify_pending(&tampered), None);
    });
}

#[quench_test::test]
fn an_expired_pending_token_does_not_verify() {
    with_key(|| {
        let expired_at = quench_core::time::Time::now().timestamp() - 1;
        let payload = format!("someone:{expired_at}");
        // `pending_key()` is private, so this mirrors it exactly: SHA-256 of
        // the shared key-encryption material, same as `mfa::pending_key`.
        let key = quench_crypto::hash::sha256(TEST_KEY_MATERIAL.as_bytes()).to_vec();
        let signature = quench_core::hex::encode(quench_crypto::hmac::sign(
            quench_crypto::hmac::Algorithm::Sha256,
            &key,
            payload.as_bytes(),
        ));
        let token = format!("{payload}:{signature}");

        assert_eq!(verify_pending(&token), None);
    });
}
