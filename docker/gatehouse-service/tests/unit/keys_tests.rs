use gatehouse_service::SigningKeys;
use gatehouse_service::test_support::TEST_KEY_MATERIAL;
use quench_auth::actix::domain::jwt::{KeyResolver, KeySigner};
use quench_db::prelude::Db;

/// `GATEHOUSE_KEY_ENCRYPTION_KEY` is a fixed env var name several modules'
/// tests in this crate need: every one of them sets it, via `envmnt::set`, to
/// this same value and never unsets it, so concurrent tests racing to set it
/// race harmlessly instead of one module's cleanup breaking another's.
async fn signing_keys(retire_after_secs: i64) -> std::sync::Arc<SigningKeys> {
    envmnt::set("GATEHOUSE_KEY_ENCRYPTION_KEY", TEST_KEY_MATERIAL);
    // Each test gets its own in-memory `Db`, so there's no cross-test
    // table state to race on.
    let db = Db::connect("").await.expect("in-memory db");
    SigningKeys::init(db, retire_after_secs)
        .await
        .expect("init")
}

#[tokio::test]
async fn init_generates_one_active_key_when_none_exist() {
    let keys = signing_keys(3600).await;
    let active = keys.active().await;
    assert!(active.is_some());

    let jwks = keys.jwks();
    assert_eq!(jwks["keys"].as_array().expect("array").len(), 1);
}

#[tokio::test]
async fn resolve_finds_the_active_keys_public_half_by_kid() {
    let keys = signing_keys(3600).await;
    let (kid, _) = keys.active().await.expect("active key");

    assert!(keys.resolve(&kid).await.is_some());
    assert!(keys.resolve("not-a-real-kid").await.is_none());
}

#[tokio::test]
async fn rotate_retires_the_previous_key_and_activates_a_new_one() {
    let keys = signing_keys(3600).await;
    let (first_kid, _) = keys.active().await.expect("active key");

    keys.rotate().await.expect("rotate");

    let (second_kid, _) = keys.active().await.expect("active key after rotate");
    assert_ne!(first_kid, second_kid);

    // The retired key is still resolvable (outstanding tokens it signed
    // must keep verifying), just no longer the one `active()` returns.
    assert!(keys.resolve(&first_kid).await.is_some());

    let jwks = keys.jwks();
    assert_eq!(jwks["keys"].as_array().expect("array").len(), 2);
}

#[tokio::test]
async fn a_key_retired_with_a_zero_grace_period_drops_out_on_reload() {
    let keys = signing_keys(0).await;
    let (first_kid, _) = keys.active().await.expect("active key");

    keys.rotate().await.expect("rotate");
    // `rotate` ends with its own `reload`, so the just-retired key (whose
    // `not_after` is already in the past with a 0s grace period) is
    // filtered out immediately rather than lingering in JWKS.
    assert!(keys.resolve(&first_kid).await.is_none());

    let jwks = keys.jwks();
    assert_eq!(jwks["keys"].as_array().expect("array").len(), 1);
}

#[tokio::test]
async fn jwks_entries_are_ed25519_okp_keys() {
    let keys = signing_keys(3600).await;
    let jwks = keys.jwks();
    let entry = &jwks["keys"][0];
    assert_eq!(entry["kty"], "OKP");
    assert_eq!(entry["crv"], "Ed25519");
    assert!(entry["kid"].is_string());
    assert!(entry["x"].is_string());
}
