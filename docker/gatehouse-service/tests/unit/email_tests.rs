use gatehouse_service::email::{LoggingSender, Sender};

#[tokio::test]
async fn logging_sender_send_verification_does_not_panic() {
    LoggingSender
        .send_verification(
            "alice@example.test",
            "alice",
            "https://example.test/verify/tok",
        )
        .await;
}

#[tokio::test]
async fn logging_sender_send_password_reset_does_not_panic() {
    LoggingSender
        .send_password_reset(
            "alice@example.test",
            "alice",
            "https://example.test/reset/tok",
        )
        .await;
}

#[tokio::test]
async fn logging_sender_is_usable_through_the_sender_trait_object() {
    let sender: Box<dyn Sender> = Box::new(LoggingSender);
    sender
        .send_verification(
            "bob@example.test",
            "bob",
            "https://example.test/verify/tok2",
        )
        .await;
}
