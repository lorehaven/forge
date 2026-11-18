use crate::support::*;
use mailroom_core::{Compose, SendRequest};
use mailroom_service::error::{Error, now};
use mailroom_service::{outgoing, worker};

macro_rules! db {
    () => {
        if !have_database() {
            return;
        }
    };
}

fn hello() -> SendRequest {
    SendRequest {
        to: vec!["alice@example.org".into()],
        subject: "Confirm your address".into(),
        text: "Follow the link.".into(),
        ..SendRequest::default()
    }
}

async fn flush(env: &Env) {
    for _ in 0..20 {
        if worker::send_due(&env.app).await.unwrap() == 0 {
            return;
        }
    }
}

#[quench_test::test]
async fn a_service_sends_through_the_estates_server_as_an_allowed_identity() {
    db!();
    let env = env().await;
    env.with_transport().await;
    let mut request = hello();
    request.html = Some("<p>Follow the <a href=\"https://x.example/v\">link</a>.</p>".into());
    request.headers = vec![("List-Unsubscribe".into(), "<https://x.example/u>".into())];
    request.idempotency_key = Some("verify-1".into());
    let queued = outgoing::send_service(&env.app, "gatehouse", &request)
        .await
        .unwrap();
    assert_eq!(queued.status, "queued");
    assert!(env.smtp.received().is_empty(), "queued, not sent inline");
    flush(&env).await;
    let got = env.smtp.received();
    assert_eq!(got.len(), 1);
    let mail = &got[0];
    assert_eq!(mail.auth_user.as_deref(), Some("noreply@example.com"));
    assert_eq!(mail.from, "noreply@example.com");
    assert_eq!(mail.to, ["alice@example.org"]);
    assert_eq!(
        mail.header("from").as_deref(),
        Some("Forge <noreply@example.com>")
    );
    assert_eq!(
        mail.header("subject").as_deref(),
        Some("Confirm your address")
    );
    assert_eq!(
        mail.header("auto-submitted").as_deref(),
        Some("auto-generated")
    );
    assert_eq!(
        mail.header("x-mailroom-source").as_deref(),
        Some("gatehouse")
    );
    assert_eq!(
        mail.header("list-unsubscribe").as_deref(),
        Some("<https://x.example/u>")
    );
    assert!(mail.text().contains("multipart/alternative"));
    // Sent: the body is dropped, the history stays.
    let page = mailroom_service::admin::outbox(&env.app, Some("sent"), 0, 10)
        .await
        .unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.items[0].source, "gatehouse");
    let row = env.app.store.outbox_get(&queued.id).await.unwrap().unwrap();
    assert!(row.raw.is_none() && row.sent.is_some());
}

#[quench_test::test]
async fn a_retry_with_the_same_key_is_not_sent_twice() {
    db!();
    let env = env().await;
    env.with_transport().await;
    let mut request = hello();
    request.idempotency_key = Some("once".into());
    let first = outgoing::send_service(&env.app, "gatehouse", &request)
        .await
        .unwrap();
    let second = outgoing::send_service(&env.app, "gatehouse", &request)
        .await
        .unwrap();
    assert_eq!(
        (second.status.as_str(), &second.id),
        ("duplicate", &first.id)
    );
    flush(&env).await;
    assert_eq!(env.smtp.received().len(), 1);
    // After it was sent, the key still answers; another service has a key space of its own.
    assert_eq!(
        outgoing::send_service(&env.app, "gatehouse", &request)
            .await
            .unwrap()
            .status,
        "duplicate"
    );
    assert_eq!(
        outgoing::send_service(&env.app, "conveyor", &request)
            .await
            .unwrap()
            .status,
        "queued"
    );
}

#[quench_test::test]
async fn a_service_cannot_send_as_just_anyone_or_to_just_anyone_in_bulk() {
    db!();
    let env = env().await;
    env.with_transport().await;
    let mut spoof = hello();
    spoof.from = "ceo@example.com".into();
    let Err(Error::Invalid(text)) = outgoing::send_service(&env.app, "gatehouse", &spoof).await
    else {
        panic!()
    };
    assert!(text.contains("not allowed"));
    let mut named = hello();
    named.from = "NOREPLY@example.com".into();
    assert!(
        outgoing::send_service(&env.app, "gatehouse", &named)
            .await
            .is_ok()
    );
    let mut bulk = hello();
    bulk.to = (0..21).map(|i| format!("u{i}@example.org")).collect();
    assert!(matches!(
        outgoing::send_service(&env.app, "gatehouse", &bulk).await,
        Err(Error::Invalid(_))
    ));
    bulk.to = vec![];
    assert!(matches!(
        outgoing::send_service(&env.app, "gatehouse", &bulk).await,
        Err(Error::Invalid(_))
    ));
    assert!(env.smtp.received().is_empty());
}

#[quench_test::test]
async fn injected_headers_and_recipients_are_refused() {
    db!();
    let env = env().await;
    env.with_transport().await;
    let cases: Vec<Box<dyn Fn(&mut SendRequest)>> = vec![
        Box::new(|r| r.subject = "Hi\r\nBcc: evil@example.net".into()),
        Box::new(|r| r.to = vec!["a@example.org\r\nBcc: evil@example.net".into()]),
        Box::new(|r| r.reply_to = Some("a@example.org\nBcc: evil@example.net".into())),
        Box::new(|r| r.headers = vec![("Bcc".into(), "evil@example.net".into())]),
        Box::new(|r| r.headers = vec![("X-Ok".into(), "a\r\nBcc: evil@example.net".into())]),
        Box::new(|r| r.headers = vec![("From".into(), "evil@example.net".into())]),
        Box::new(|r| r.idempotency_key = Some("k\r\nx".into())),
        Box::new(|r| r.idempotency_key = Some("k".repeat(200))),
        Box::new(|r| r.text = "x".repeat(1024 * 1024 + 1)),
        Box::new(|r| r.html = Some("x".repeat(2 * 1024 * 1024 + 1))),
    ];
    for mutate in cases {
        let mut request = hello();
        mutate(&mut request);
        assert!(matches!(
            outgoing::send_service(&env.app, "gatehouse", &request).await,
            Err(Error::Invalid(_))
        ));
    }
    flush(&env).await;
    assert!(env.smtp.received().is_empty());
}

#[quench_test::test]
async fn without_a_sending_server_there_is_nothing_to_queue() {
    db!();
    let env = env().await;
    let Err(Error::Upstream(text)) = outgoing::send_service(&env.app, "gatehouse", &hello()).await
    else {
        panic!()
    };
    assert!(text.contains("no sending server"));
    assert_eq!(
        env.app
            .store
            .outbox_count_since("", "service", 0)
            .await
            .unwrap(),
        0
    );
}

#[quench_test::test]
async fn hourly_and_daily_budgets_hold() {
    db!();
    let env = env().await;
    env.with_transport().await;
    let mut settings = env.app.settings();
    settings.hourly_per_source = 3;
    settings.daily_budget = 5;
    env.app.set_settings(settings);
    for _ in 0..3 {
        outgoing::send_service(&env.app, "a", &hello())
            .await
            .unwrap();
    }
    assert!(matches!(
        outgoing::send_service(&env.app, "a", &hello()).await,
        Err(Error::Limited(_))
    ));
    // Another source has its own hour, but the day's budget is the estate's.
    outgoing::send_service(&env.app, "b", &hello())
        .await
        .unwrap();
    outgoing::send_service(&env.app, "b", &hello())
        .await
        .unwrap();
    assert!(matches!(
        outgoing::send_service(&env.app, "b", &hello()).await,
        Err(Error::Limited(_))
    ));
}

#[quench_test::test]
async fn a_busy_server_is_retried_later_and_a_refusal_is_final() {
    db!();
    let env = env().await;
    env.with_transport().await;
    env.smtp.fail_next_data(451, "4.3.0 try again later");
    let queued = outgoing::send_service(&env.app, "gatehouse", &hello())
        .await
        .unwrap();
    flush(&env).await;
    let row = env.app.store.outbox_get(&queued.id).await.unwrap().unwrap();
    assert_eq!((row.status.as_str(), row.attempts), ("queued", 1));
    assert!(row.next_attempt.unwrap() > now() + 30);
    assert!(row.last_error.unwrap().contains("451"));
    assert!(env.smtp.received().is_empty());
    // Retry right away from the panel: it goes.
    mailroom_service::admin::retry(&env.app, "root", &queued.id)
        .await
        .unwrap();
    flush(&env).await;
    assert_eq!(
        env.app
            .store
            .outbox_get(&queued.id)
            .await
            .unwrap()
            .unwrap()
            .status,
        "sent"
    );
    assert_eq!(env.smtp.received().len(), 1);
}

#[quench_test::test]
async fn a_message_gives_up_after_enough_failures_and_keeps_no_body() {
    db!();
    let env = env().await;
    env.with_transport().await;
    let queued = outgoing::send_service(&env.app, "gatehouse", &hello())
        .await
        .unwrap();
    for attempt in 1..=worker::MAX_ATTEMPTS {
        env.smtp.fail_next_data(451, "busy");
        env.app
            .store
            .pool
            .execute(
                "UPDATE outbox SET next_attempt = 0 WHERE id = $1",
                &[&queued.id],
            )
            .await
            .unwrap();
        worker::send_due(&env.app).await.unwrap();
        let row = env.app.store.outbox_get(&queued.id).await.unwrap().unwrap();
        if attempt < worker::MAX_ATTEMPTS {
            assert_eq!(row.status, "queued", "attempt {attempt}");
        } else {
            assert_eq!(row.status, "failed");
            assert!(row.raw.is_none(), "a failed message does not stay readable");
        }
    }
    assert!(env.smtp.received().is_empty());
    let stats = mailroom_service::admin::stats(&env.app).await.unwrap();
    assert_eq!((stats.queued, stats.failed_24h, stats.sent_24h), (0, 1, 0));
}

#[quench_test::test]
async fn a_wrong_relay_password_fails_at_once_with_a_readable_reason() {
    db!();
    let env = env().await;
    env.with_transport().await;
    mailroom_service::settings::save_transport_secret(
        &env.app.store,
        &env.app.vault,
        "not-the-password",
    )
    .await
    .unwrap();
    let queued = outgoing::send_service(&env.app, "gatehouse", &hello())
        .await
        .unwrap();
    flush(&env).await;
    let row = env.app.store.outbox_get(&queued.id).await.unwrap().unwrap();
    assert_eq!((row.status.as_str(), row.attempts), ("failed", 1));
    assert!(
        row.last_error
            .unwrap()
            .to_lowercase()
            .contains("authentication")
    );
}

#[quench_test::test]
async fn messages_stuck_in_sending_are_put_back() {
    db!();
    let env = env().await;
    env.with_transport().await;
    let queued = outgoing::send_service(&env.app, "gatehouse", &hello())
        .await
        .unwrap();
    let claimed = env.app.store.outbox_claim(now(), 5).await.unwrap();
    assert_eq!(claimed.len(), 1);
    assert!(
        env.app
            .store
            .outbox_claim(now(), 5)
            .await
            .unwrap()
            .is_empty(),
        "claimed once"
    );
    env.app
        .store
        .pool
        .execute(
            "UPDATE outbox SET next_attempt = 1 WHERE id = $1",
            &[&queued.id],
        )
        .await
        .unwrap();
    assert_eq!(
        env.app
            .store
            .outbox_requeue_stuck(now() - 600)
            .await
            .unwrap(),
        1
    );
    flush(&env).await;
    assert_eq!(env.smtp.received().len(), 1);
}

fn mail(account: &str, to: &str) -> Compose {
    Compose {
        account_id: account.to_string(),
        to: vec![to.to_string()],
        subject: "Hello from Mailroom".into(),
        text: "Body of the message".into(),
        ..Compose::default()
    }
}

#[quench_test::test]
async fn a_person_sends_from_their_own_mailbox_and_keeps_a_copy() {
    db!();
    let env = env().await;
    let account = env.add_account(OWNER).await;
    let queued = outgoing::send_user(&env.app, OWNER, &mail(&account.id, "friend@example.org"))
        .await
        .unwrap();
    assert_eq!(queued.status, "queued");
    flush(&env).await;
    let got = env.smtp.received();
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].auth_user.as_deref(), Some("me@example.com"));
    assert_eq!(got[0].from, "me@example.com");
    assert_eq!(got[0].header("from").as_deref(), Some("me@example.com"));
    assert!(got[0].text().contains("Body of the message"));
    let sent = env.imap.messages("me@example.com", "Sent");
    assert_eq!(sent.len(), 1, "a copy in Sent");
    assert!(sent[0].flags.contains(&"\\Seen".to_string()));
    assert_eq!(
        env.app
            .store
            .outbox_get(&queued.id)
            .await
            .unwrap()
            .unwrap()
            .status,
        "sent"
    );
}

#[quench_test::test]
async fn providers_that_keep_their_own_copy_are_not_given_a_second() {
    db!();
    let env = env().await;
    let mut input = env.input();
    input.save_sent = false;
    let account = mailroom_service::accounts::create(&env.app, OWNER, &input)
        .await
        .unwrap();
    mailroom_service::sync::sync_account(&env.app, &account.id)
        .await
        .unwrap();
    outgoing::send_user(&env.app, OWNER, &mail(&account.id, "friend@example.org"))
        .await
        .unwrap();
    flush(&env).await;
    assert_eq!(env.smtp.received().len(), 1);
    assert!(env.imap.messages("me@example.com", "Sent").is_empty());
}

#[quench_test::test]
async fn a_reply_is_threaded_and_marks_the_original_answered() {
    db!();
    let env = env().await;
    env.deliver("INBOX", "question", "what time?");
    let account = env.add_account(OWNER).await;
    let inbox = env.folder(&account.id, "inbox").await;
    let original = mailroom_service::mailbox::list(
        &env.app,
        OWNER,
        &mailroom_service::mailbox::ListSpec {
            folder_id: Some(inbox.id),
            limit: 5,
            ..Default::default()
        },
    )
    .await
    .unwrap()
    .items
    .remove(0);
    let mut reply = mail(&account.id, "alice@x.example");
    reply.subject = "Re: question".into();
    reply.in_reply_to = Some(original.id.clone());
    outgoing::send_user(&env.app, OWNER, &reply).await.unwrap();
    flush(&env).await;
    let got = &env.smtp.received()[0];
    assert_eq!(
        got.header("in-reply-to").as_deref(),
        Some("<question@x.example>")
    );
    assert_eq!(
        got.header("references").as_deref(),
        Some("<question@x.example>")
    );
    assert!(
        env.imap.messages("me@example.com", "INBOX")[0]
            .flags
            .contains(&"\\Answered".to_string())
    );
    let view = mailroom_service::mailbox::open(&env.app, OWNER, &original.id)
        .await
        .unwrap();
    assert!(view.summary.answered);
    // Someone else's message cannot be replied to.
    let mut foreign = mail(&account.id, "x@example.org");
    foreign.in_reply_to = Some(original.id);
    assert!(matches!(
        outgoing::send_user(&env.app, "bob", &foreign).await,
        Err(Error::NotFound)
    ));
}

#[quench_test::test]
async fn attachments_are_uploaded_then_sent_and_the_upload_is_consumed() {
    db!();
    let env = env().await;
    let account = env.add_account(OWNER).await;
    let up = outgoing::upload(
        &env.app,
        OWNER,
        "../../zażółć.txt",
        "text/plain",
        b"file contents".to_vec(),
    )
    .await
    .unwrap();
    assert_eq!((up.filename.as_str(), up.size), ("zażółć.txt", 13));
    let mut request = mail(&account.id, "friend@example.org");
    request.attachments = vec![up.id.clone()];
    outgoing::send_user(&env.app, OWNER, &request)
        .await
        .unwrap();
    flush(&env).await;
    let message = mail_client::message::parse(&env.smtp.received()[0].data);
    assert_eq!(message.attachments.len(), 1);
    assert_eq!(message.attachments[0].filename, "zażółć.txt");
    assert!(
        env.app
            .store
            .uploads_owned(OWNER, &[up.id.clone()])
            .await
            .unwrap()
            .is_empty(),
        "consumed"
    );
    // The same upload cannot be sent again, and nobody else can attach it.
    assert!(matches!(
        outgoing::send_user(&env.app, OWNER, &request).await,
        Err(Error::Invalid(_))
    ));
    let again = outgoing::upload(&env.app, OWNER, "a.txt", "text/plain", b"x".to_vec())
        .await
        .unwrap();
    let mut theirs = mail(&account.id, "x@example.org");
    theirs.attachments = vec![again.id];
    assert!(matches!(
        outgoing::send_user(&env.app, "bob", &theirs).await,
        Err(Error::NotFound)
    ));
}

#[quench_test::test]
async fn uploads_are_bounded() {
    db!();
    let env = env().await;
    assert!(matches!(
        outgoing::upload(&env.app, OWNER, "e", "x/y", vec![]).await,
        Err(Error::Invalid(_))
    ));
    assert!(matches!(
        outgoing::upload(
            &env.app,
            OWNER,
            "big",
            "x/y",
            vec![0; outgoing::MAX_UPLOAD + 1]
        )
        .await,
        Err(Error::Invalid(_))
    ));
    for i in 0..3 {
        outgoing::upload(
            &env.app,
            OWNER,
            &format!("f{i}"),
            "x/y",
            vec![1; outgoing::MAX_UPLOAD],
        )
        .await
        .unwrap();
    }
    let Err(Error::Invalid(text)) =
        outgoing::upload(&env.app, OWNER, "one more", "x/y", vec![1; 1024]).await
    else {
        panic!()
    };
    assert!(text.contains("waiting"));
    assert!(
        outgoing::upload(&env.app, "bob", "mine", "x/y", vec![1; 1024])
            .await
            .is_ok()
    );
    assert_eq!(env.app.store.uploads_prune(now() + 10).await.unwrap(), 4);
}

#[quench_test::test]
async fn a_persons_mail_is_checked_and_rate_limited() {
    db!();
    let env = env().await;
    let account = env.add_account(OWNER).await;
    let mut bad = mail(&account.id, "not an address");
    assert!(matches!(
        outgoing::send_user(&env.app, OWNER, &bad).await,
        Err(Error::Invalid(_))
    ));
    bad.to = vec![];
    assert!(matches!(
        outgoing::send_user(&env.app, OWNER, &bad).await,
        Err(Error::Invalid(_))
    ));
    bad = mail(&account.id, "a@example.org");
    bad.subject = "x\r\nBcc: evil@example.net".into();
    assert!(matches!(
        outgoing::send_user(&env.app, OWNER, &bad).await,
        Err(Error::Invalid(_))
    ));
    bad = mail(&account.id, "a@example.org");
    bad.text = "x".repeat(1024 * 1024 + 1);
    assert!(matches!(
        outgoing::send_user(&env.app, OWNER, &bad).await,
        Err(Error::Invalid(_))
    ));
    // Not their mailbox.
    assert!(matches!(
        outgoing::send_user(&env.app, "bob", &mail(&account.id, "a@example.org")).await,
        Err(Error::NotFound)
    ));
    // A disabled one.
    mailroom_service::admin::set_account_disabled(&env.app, "root", &account.id, true)
        .await
        .unwrap();
    assert!(matches!(
        outgoing::send_user(&env.app, OWNER, &mail(&account.id, "a@example.org")).await,
        Err(Error::Forbidden)
    ));
    mailroom_service::admin::set_account_disabled(&env.app, "root", &account.id, false)
        .await
        .unwrap();
    // Sixty an hour.
    let mut limited = false;
    for _ in 0..70 {
        if matches!(
            outgoing::send_user(&env.app, OWNER, &mail(&account.id, "a@example.org")).await,
            Err(Error::Limited(_))
        ) {
            limited = true;
            break;
        }
    }
    assert!(limited);
}

#[quench_test::test]
async fn the_signature_follows_the_message() {
    db!();
    let env = env().await;
    let mut input = env.input();
    input.signature = "Ann\nWorkshop".into();
    let account = mailroom_service::accounts::create(&env.app, OWNER, &input)
        .await
        .unwrap();
    outgoing::send_user(&env.app, OWNER, &mail(&account.id, "a@example.org"))
        .await
        .unwrap();
    flush(&env).await;
    let text = mail_client::message::parse(&env.smtp.received()[0].data).text;
    assert!(
        text.contains("Body of the message") && text.contains("-- ") && text.contains("Workshop"),
        "{text}"
    );
}
