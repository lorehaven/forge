use gatehouse_service::VerificationTokens;
use gatehouse_service::tokens::{PURPOSE_RESET_PASSWORD, PURPOSE_VERIFY_EMAIL};

#[tokio::test]
async fn issue_then_redeem_round_trips_the_username() {
    let tokens = VerificationTokens::in_memory();
    let token = tokens
        .issue(PURPOSE_VERIFY_EMAIL, "alice", 60)
        .await
        .expect("issue");

    let redeemed = tokens
        .redeem(PURPOSE_VERIFY_EMAIL, &token)
        .await
        .expect("redeem");
    assert_eq!(redeemed, Some("alice".to_string()));
}

#[tokio::test]
async fn redeem_is_single_use() {
    let tokens = VerificationTokens::in_memory();
    let token = tokens
        .issue(PURPOSE_RESET_PASSWORD, "bob", 60)
        .await
        .expect("issue");

    assert!(
        tokens
            .redeem(PURPOSE_RESET_PASSWORD, &token)
            .await
            .expect("first redeem")
            .is_some()
    );
    assert!(
        tokens
            .redeem(PURPOSE_RESET_PASSWORD, &token)
            .await
            .expect("second redeem")
            .is_none()
    );
}

#[tokio::test]
async fn redeem_is_scoped_to_its_purpose() {
    let tokens = VerificationTokens::in_memory();
    let token = tokens
        .issue(PURPOSE_VERIFY_EMAIL, "carol", 60)
        .await
        .expect("issue");

    // A token minted for verification must not redeem as a password
    // reset - see the module docs on why `purpose` is folded into the key.
    assert!(
        tokens
            .redeem(PURPOSE_RESET_PASSWORD, &token)
            .await
            .expect("redeem")
            .is_none()
    );
    assert!(
        tokens
            .redeem(PURPOSE_VERIFY_EMAIL, &token)
            .await
            .expect("redeem")
            .is_some()
    );
}

#[tokio::test]
async fn redeem_of_an_unknown_token_is_none() {
    let tokens = VerificationTokens::in_memory();
    assert!(
        tokens
            .redeem(PURPOSE_VERIFY_EMAIL, "never-issued")
            .await
            .expect("redeem")
            .is_none()
    );
}

#[tokio::test]
async fn issued_tokens_are_unique() {
    let tokens = VerificationTokens::in_memory();
    let a = tokens
        .issue(PURPOSE_VERIFY_EMAIL, "dana", 60)
        .await
        .expect("issue");
    let b = tokens
        .issue(PURPOSE_VERIFY_EMAIL, "dana", 60)
        .await
        .expect("issue");
    assert_ne!(a, b);
}
