//! Opt-in checks against a real mail server. Ignored by default; run with the
//! settings in `LIVE_*` variables (same names as production, `LIVE_` prefixed):
//!
//! ```text
//! LIVE_SMTP_HOST=192.168.0.9 LIVE_SMTP_TLS_SERVER_NAME=ennor.ddns.net \
//! LIVE_SMTP_USERNAME=noreply@ennor.ddns.net LIVE_SMTP_PASSWORD=... \
//! LIVE_MAIL_FROM=noreply@ennor.ddns.net LIVE_SMTP_TO=admin@ennor.ddns.net \
//!   cargo test -p gatehouse-service --test unit live -- --ignored --nocapture
//! ```

use gatehouse_service::email::{Recipient, Sender, SmtpConfig, SmtpSender};

fn live_sender() -> SmtpSender {
    let config = SmtpConfig::from_lookup(&|key| std::env::var(format!("LIVE_{key}")).ok())
        .expect("valid LIVE_* configuration")
        .expect("LIVE_SMTP_HOST must be set");
    SmtpSender::new(config).expect("sender")
}

#[tokio::test]
#[ignore = "needs a real SMTP server (LIVE_* variables)"]
async fn live_login_is_accepted() {
    // Connects, verifies the certificate, logs in, hangs up - sends nothing.
    let sender = live_sender();
    sender.spawn_startup_check();
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
}

#[tokio::test]
#[ignore = "sends a real email (LIVE_* variables)"]
async fn live_verification_email_is_accepted() {
    let to = std::env::var("LIVE_SMTP_TO").expect("LIVE_SMTP_TO must be set");
    let recipient = Recipient {
        address: &to,
        username: "live-test",
        locale: Some("pl-PL"),
    };
    live_sender()
        .send_verification(
            &recipient,
            "https://ennor.ddns.net/gatehouse/ui/verify?token=live-test",
        )
        .await
        .expect("the server accepted the message");
}
