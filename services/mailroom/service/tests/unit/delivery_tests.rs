//! Mail between people and from services, into real Postgres.

use crate::support::*;
use mailroom_core::{Compose, SendRequest};
use mailroom_service::delivery::{self, Author, File, Letter};
use mailroom_service::error::Error;
use mailroom_service::{mailbox, outgoing};

macro_rules! db {
    () => {
        if !have_database() {
            return;
        }
    };
}

#[quench_test::test]
async fn a_message_goes_to_every_recipient_and_a_copy_stays_in_sent() {
    db!();
    let env = env().await;
    env.people(&["ann", "bob", "cy"]).await;
    let sent = env
        .write(
            "ann",
            &["bob", "cy@home.example"],
            "Lunch?",
            "Noon at the usual place",
        )
        .await;
    assert_eq!((sent.status.as_str(), sent.delivered), ("delivered", 2));
    for who in ["bob", "cy"] {
        let inbox = env.folder(who, "inbox").await;
        assert_eq!(inbox.len(), 1, "{who}");
        assert_eq!(inbox[0].subject, "Lunch?");
        assert_eq!(inbox[0].from.address, "ann@home.example");
        assert!(!inbox[0].seen && !inbox[0].automated);
        let to: Vec<_> = inbox[0].to.iter().map(|c| c.address.as_str()).collect();
        assert_eq!(to, ["bob@home.example", "cy@home.example"]);
    }
    let sent_folder = env.folder("ann", "sent").await;
    assert_eq!(sent_folder.len(), 1);
    assert_eq!(sent_folder[0].id, sent.id);
    assert!(sent_folder[0].seen);
    assert!(env.folder("ann", "inbox").await.is_empty());
}

#[quench_test::test]
async fn bcc_recipients_get_it_without_being_named() {
    db!();
    let env = env().await;
    env.people(&["ann", "bob", "cy"]).await;
    mailroom_service::outgoing::send_user(
        &env.app,
        "ann",
        &Compose {
            to: vec!["bob".into()],
            bcc: vec!["cy".into()],
            subject: "s".into(),
            text: "t".into(),
            ..Compose::default()
        },
    )
    .await
    .unwrap();
    let seen_by_cy = env.folder("cy", "inbox").await;
    assert_eq!(seen_by_cy.len(), 1);
    let to: Vec<_> = seen_by_cy[0]
        .to
        .iter()
        .map(|c| c.address.as_str())
        .collect();
    assert_eq!(to, ["bob@home.example"], "the blind copy is not listed");
    let view = mailbox::open(&env.app, "bob", &env.folder("bob", "inbox").await[0].id)
        .await
        .unwrap();
    assert!(view.cc.is_empty());
}

#[quench_test::test]
async fn a_person_cannot_write_to_someone_without_a_mailbox_or_outside_the_estate() {
    db!();
    let env = env().await;
    env.people(&["ann", "bob"]).await;
    for (to, needle) in [
        ("ghost", "has no mailbox"),
        ("bob@gmail.com", "mail stays inside this estate"),
        ("not an address", "not a user name"),
    ] {
        let e = outgoing::send_user(
            &env.app,
            "ann",
            &Compose {
                to: vec![to.into()],
                subject: "s".into(),
                text: "t".into(),
                ..Compose::default()
            },
        )
        .await
        .unwrap_err();
        assert!(
            matches!(&e, Error::Invalid(m) if m.contains(needle)),
            "{to}: {e:?}"
        );
    }
    // One missing recipient refuses the whole message: nobody gets half of it.
    let e = outgoing::send_user(
        &env.app,
        "ann",
        &Compose {
            to: vec!["bob".into(), "ghost".into()],
            subject: "s".into(),
            text: "t".into(),
            ..Compose::default()
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(e, Error::Invalid(_)));
    assert!(env.folder("bob", "inbox").await.is_empty());
    assert!(env.folder("ann", "sent").await.is_empty());
}

#[quench_test::test]
async fn a_full_mailbox_refuses_and_the_message_is_not_half_delivered() {
    db!();
    let env = env().await;
    env.people(&["ann", "bob", "cy"]).await;
    let mut settings = env.app.settings();
    settings.quota_mb = 1;
    env.app.set_settings(settings);
    // Fill cy's mailbox with most of its megabyte.
    env.write("ann", &["cy"], "big", &"x".repeat(900_000)).await;
    let e = outgoing::send_user(
        &env.app,
        "ann",
        &Compose {
            to: vec!["bob".into(), "cy".into()],
            subject: "more".into(),
            text: "y".repeat(300_000),
            ..Compose::default()
        },
    )
    .await
    .unwrap_err();
    assert!(
        matches!(&e, Error::Invalid(m) if m.contains("cy@home.example mailbox is full")),
        "{e:?}"
    );
    assert!(
        env.folder("bob", "inbox").await.is_empty(),
        "nobody got half of it"
    );
    assert_eq!(env.folder("cy", "inbox").await.len(), 1);
}

#[quench_test::test]
async fn a_reply_is_threaded_and_marks_the_original_answered() {
    db!();
    let env = env().await;
    env.people(&["ann", "bob"]).await;
    env.write("ann", &["bob"], "Plan", "Shall we?").await;
    let original = env.folder("bob", "inbox").await.remove(0);
    let sent = outgoing::send_user(
        &env.app,
        "bob",
        &Compose {
            to: vec!["ann".into()],
            subject: "Re: Plan".into(),
            text: "Yes".into(),
            in_reply_to: Some(original.id.clone()),
            ..Compose::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(sent.delivered, 1);
    assert!(env.folder("bob", "inbox").await[0].answered);
    let reply = env.folder("ann", "inbox").await.remove(0);
    let row = env
        .app
        .store
        .message_get_owned(&reply.id, "ann")
        .await
        .unwrap()
        .unwrap();
    let first = env
        .app
        .store
        .message_get_owned(&original.id, "bob")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.in_reply_to, first.message_id);
    // Answering somebody else's message is not possible.
    let e = outgoing::send_user(
        &env.app,
        "bob",
        &Compose {
            to: vec!["ann".into()],
            subject: "x".into(),
            text: "x".into(),
            in_reply_to: Some(env.folder("ann", "inbox").await[0].id.clone()),
            ..Compose::default()
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(e, Error::NotFound));
}

#[quench_test::test]
async fn attachments_are_stored_once_and_every_copy_can_read_them() {
    db!();
    let env = env().await;
    env.people(&["ann", "bob", "cy"]).await;
    let up = outgoing::upload(
        &env.app,
        "ann",
        "notes.txt",
        "text/plain",
        b"hello".to_vec(),
    )
    .await
    .unwrap();
    outgoing::send_user(
        &env.app,
        "ann",
        &Compose {
            to: vec!["bob".into(), "cy".into()],
            subject: "files".into(),
            text: "see attached".into(),
            attachments: vec![up.id.clone()],
            ..Compose::default()
        },
    )
    .await
    .unwrap();
    // The upload was consumed; the file is in all three copies, as one blob.
    assert!(
        env.app
            .store
            .uploads_owned("ann", &[up.id])
            .await
            .unwrap()
            .is_empty()
    );
    for who in ["bob", "cy", "ann"] {
        let folder = if who == "ann" { "sent" } else { "inbox" };
        let m = env.folder(who, folder).await.remove(0);
        assert!(m.attachments);
        let (name, data) = mailbox::attachment(&env.app, who, &m.id, 0).await.unwrap();
        assert_eq!(
            (name.as_str(), data.as_slice()),
            ("notes.txt", b"hello".as_slice())
        );
        let view = mailbox::open(&env.app, who, &m.id).await.unwrap();
        assert_eq!(view.attachments.len(), 1);
        assert_eq!(view.attachments[0].filename, "notes.txt");
    }
    let blobs = env
        .app
        .store
        .pool
        .query_one("SELECT COUNT(*)::BIGINT AS n FROM blobs", &[])
        .await
        .unwrap();
    let n: i64 = blobs.get("n").unwrap();
    assert_eq!(n, 1);
    // Somebody else's upload cannot be attached.
    let theirs = outgoing::upload(&env.app, "bob", "b.txt", "text/plain", b"b".to_vec())
        .await
        .unwrap();
    let e = outgoing::send_user(
        &env.app,
        "ann",
        &Compose {
            to: vec!["cy".into()],
            subject: "s".into(),
            text: "t".into(),
            attachments: vec![theirs.id],
            ..Compose::default()
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(e, Error::NotFound));
}

#[quench_test::test]
async fn a_service_writes_to_people_who_have_not_signed_in_yet() {
    db!();
    let env = env().await;
    let sent = env
        .service_mail(
            "gatehouse-mailroom",
            &["newcomer", "other@home.example"],
            "Welcome",
            "Hello!",
        )
        .await
        .unwrap();
    assert_eq!((sent.status.as_str(), sent.delivered), ("delivered", 2));
    let inbox = env.folder("newcomer", "inbox").await;
    assert_eq!(inbox[0].from.address, "gatehouse@home.example");
    assert_eq!(inbox[0].from.name, "Gatehouse");
    assert!(inbox[0].automated);
    // The mailbox that was made is theirs when they arrive.
    let me = mailbox::me(&env.app, "newcomer", true, false)
        .await
        .unwrap();
    assert_eq!(me.address, "newcomer@home.example");
    // Nobody can answer a service: it has no mailbox.
    env.people(&["ann"]).await;
    let e = outgoing::send_user(
        &env.app,
        "ann",
        &Compose {
            to: vec!["gatehouse".into()],
            subject: "Re".into(),
            text: "?".into(),
            ..Compose::default()
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(&e, Error::Invalid(m) if m.contains("no mailbox")));
}

#[quench_test::test]
async fn a_service_message_reaches_whoever_has_room_and_says_so() {
    db!();
    let env = env().await;
    env.people(&["full", "roomy"]).await;
    let mut settings = env.app.settings();
    settings.quota_mb = 1;
    env.app.set_settings(settings);
    env.service_mail("conveyor", &["full"], "big", &"x".repeat(900_000))
        .await
        .unwrap();
    let sent = env
        .service_mail(
            "conveyor",
            &["full", "roomy"],
            &"y".repeat(10),
            &"z".repeat(300_000),
        )
        .await
        .unwrap();
    assert_eq!((sent.status.as_str(), sent.delivered), ("partial", 1));
    assert_eq!(env.folder("roomy", "inbox").await.len(), 1);
    assert_eq!(env.folder("full", "inbox").await.len(), 1);
    // Nobody could take it: an error, and the log says why.
    let e = env
        .service_mail("conveyor", &["full"], "again", &"z".repeat(300_000))
        .await
        .unwrap_err();
    assert!(
        matches!(&e, Error::Invalid(m) if m.contains("nobody received")),
        "{e:?}"
    );
    let log = mailroom_service::admin::deliveries(&env.app, 0, 10)
        .await
        .unwrap();
    assert_eq!(log.items[0].status, "failed");
    assert!(log.items[0].error.as_deref().unwrap_or("").contains("full"));
    assert_eq!(log.items[1].status, "partial");
    assert_eq!(log.items[2].status, "delivered");
}

#[quench_test::test]
async fn a_retry_with_the_same_key_delivers_nothing_twice() {
    db!();
    let env = env().await;
    let request = SendRequest {
        to: vec!["ann".into()],
        subject: "once".into(),
        text: "only once".into(),
        idempotency_key: Some("run-42".into()),
        kind: Some("notification".into()),
        unsubscribe: Some("https://example.test/unsub?t=1".into()),
        ..SendRequest::default()
    };
    let first = outgoing::send_service(&env.app, "gatehouse-mailroom", &request)
        .await
        .unwrap();
    let again = outgoing::send_service(&env.app, "gatehouse-mailroom", &request)
        .await
        .unwrap();
    assert_eq!(
        (first.status.as_str(), again.status.as_str()),
        ("delivered", "duplicate")
    );
    assert_eq!((again.id.as_str(), again.delivered), (first.id.as_str(), 1));
    assert_eq!(env.folder("ann", "inbox").await.len(), 1);
    // Another service may use the same key.
    let other = outgoing::send_service(&env.app, "conveyor-mailroom", &request)
        .await
        .unwrap();
    assert_eq!(other.status, "delivered");
    assert_eq!(env.folder("ann", "inbox").await.len(), 2);
    let view = mailbox::open(&env.app, "ann", &env.folder("ann", "inbox").await[0].id)
        .await
        .unwrap();
    assert_eq!(view.unsubscribe, "https://example.test/unsub?t=1");
    // A failed attempt frees its key.
    let bad = SendRequest {
        subject: "a\nb".into(),
        idempotency_key: Some("k-bad".into()),
        ..request.clone()
    };
    assert!(
        outgoing::send_service(&env.app, "gatehouse", &bad)
            .await
            .is_err()
    );
    let good = SendRequest {
        subject: "ab".into(),
        ..bad
    };
    assert_eq!(
        outgoing::send_service(&env.app, "gatehouse", &good)
            .await
            .unwrap()
            .status,
        "delivered"
    );
}

#[quench_test::test]
async fn service_requests_are_checked_and_budgeted() {
    db!();
    let env = env().await;
    let base = SendRequest {
        to: vec!["ann".into()],
        subject: "s".into(),
        text: "t".into(),
        ..SendRequest::default()
    };
    for (what, request) in [
        (
            "no recipient",
            SendRequest {
                to: vec![],
                ..base.clone()
            },
        ),
        (
            "outside",
            SendRequest {
                to: vec!["a@gmail.com".into()],
                ..base.clone()
            },
        ),
        (
            "odd name",
            SendRequest {
                to: vec!["a b".into()],
                ..base.clone()
            },
        ),
        (
            "empty",
            SendRequest {
                text: String::new(),
                ..base.clone()
            },
        ),
        (
            "long key",
            SendRequest {
                idempotency_key: Some("k".repeat(200)),
                ..base.clone()
            },
        ),
        (
            "link",
            SendRequest {
                unsubscribe: Some("a b".into()),
                ..base.clone()
            },
        ),
        (
            "nul",
            SendRequest {
                text: "a\0b".into(),
                ..base.clone()
            },
        ),
        (
            "many",
            SendRequest {
                to: (0..60).map(|i| format!("u{i}")).collect(),
                ..base.clone()
            },
        ),
    ] {
        let e = outgoing::send_service(&env.app, "gatehouse", &request)
            .await
            .unwrap_err();
        assert!(matches!(e, Error::Invalid(_)), "{what}: {e:?}");
    }
    let mut settings = env.app.settings();
    settings.hourly_per_source = 2;
    settings.daily_budget = 3;
    env.app.set_settings(settings);
    for _ in 0..2 {
        outgoing::send_service(&env.app, "svc-a", &base)
            .await
            .unwrap();
    }
    assert!(matches!(
        outgoing::send_service(&env.app, "svc-a", &base)
            .await
            .unwrap_err(),
        Error::Limited(_)
    ));
    // Another service has its own hourly allowance but shares the day's budget.
    outgoing::send_service(&env.app, "svc-b", &base)
        .await
        .unwrap();
    assert!(matches!(
        outgoing::send_service(&env.app, "svc-b", &base)
            .await
            .unwrap_err(),
        Error::Limited(_)
    ));
}

#[quench_test::test]
async fn a_person_is_rate_limited_and_a_disabled_mailbox_still_receives() {
    db!();
    let env = env().await;
    env.people(&["ann", "bob"]).await;
    let mut settings = env.app.settings();
    settings.hourly_per_person = 2;
    env.app.set_settings(settings);
    env.write("ann", &["bob"], "1", "t").await;
    env.write("ann", &["bob"], "2", "t").await;
    let e = outgoing::send_user(
        &env.app,
        "ann",
        &Compose {
            to: vec!["bob".into()],
            subject: "3".into(),
            text: "t".into(),
            ..Compose::default()
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(e, Error::Limited(_)));
    mailroom_service::admin::set_disabled(&env.app, "adam", "bob", true)
        .await
        .unwrap();
    // The disabled person's mailbox still receives mail, and the service's too, but they cannot open it or send from it.
    env.service_mail("gatehouse", &["bob"], "while disabled", "t")
        .await
        .unwrap();
    assert!(matches!(
        mailbox::folders(&env.app, "bob").await.unwrap_err(),
        Error::Forbidden
    ));
    assert!(matches!(
        outgoing::send_user(
            &env.app,
            "bob",
            &Compose {
                to: vec!["ann".into()],
                subject: "s".into(),
                text: "t".into(),
                ..Compose::default()
            },
        )
        .await
        .unwrap_err(),
        Error::Forbidden
    ));
    mailroom_service::admin::set_disabled(&env.app, "adam", "bob", false)
        .await
        .unwrap();
    assert_eq!(env.folder("bob", "inbox").await.len(), 3);
}

#[quench_test::test]
async fn letters_are_checked_before_anything_is_written() {
    let ok = Letter {
        to: vec!["ann".into()],
        subject: "s".into(),
        text: "t".into(),
        ..Letter::default()
    };
    assert!(delivery::check(&ok).is_ok());
    let cases: Vec<Letter> = vec![
        Letter {
            subject: "x".repeat(301),
            ..ok.clone()
        },
        Letter {
            subject: "a\r\nBcc: x".into(),
            ..ok.clone()
        },
        Letter {
            text: "x".repeat(delivery::MAX_TEXT + 1),
            ..ok.clone()
        },
        Letter {
            html: Some("x".repeat(delivery::MAX_HTML + 1)),
            ..ok.clone()
        },
        Letter {
            html: Some("a\0".into()),
            ..ok.clone()
        },
        Letter {
            text: "  ".into(),
            ..ok.clone()
        },
        Letter {
            unsubscribe: "x".repeat(2001),
            ..ok.clone()
        },
        Letter {
            files: (0..=delivery::MAX_FILES)
                .map(|i| File {
                    filename: format!("{i}"),
                    media_type: "a/b".into(),
                    data: vec![1],
                })
                .collect(),
            ..ok.clone()
        },
        Letter {
            files: vec![File {
                filename: "big".into(),
                media_type: "a/b".into(),
                data: vec![0; delivery::MAX_ATTACHMENTS + 1],
            }],
            ..ok.clone()
        },
    ];
    for (i, letter) in cases.iter().enumerate() {
        assert!(delivery::check(letter).is_err(), "case {i}");
    }
    // HTML alone, or a file alone, is a message.
    assert!(
        delivery::check(&Letter {
            text: String::new(),
            html: Some("<p>x</p>".into()),
            ..ok.clone()
        })
        .is_ok()
    );
    assert!(
        delivery::check(&Letter {
            text: String::new(),
            files: vec![File {
                filename: "f".into(),
                media_type: "a/b".into(),
                data: vec![1]
            }],
            ..ok
        })
        .is_ok()
    );
}

#[quench_test::test]
async fn html_only_mail_gets_a_text_version_and_a_preview() {
    db!();
    let env = env().await;
    let request = SendRequest {
        to: vec!["ann".into()],
        subject: "html".into(),
        text: String::new(),
        html: Some("<h1>Hello</h1><p>World <b>again</b></p>".into()),
        ..SendRequest::default()
    };
    outgoing::send_service(&env.app, "gatehouse", &request)
        .await
        .unwrap();
    let m = env.folder("ann", "inbox").await.remove(0);
    assert!(
        m.preview.contains("Hello") && m.preview.contains("World"),
        "{}",
        m.preview
    );
    let view = mailbox::open(&env.app, "ann", &m.id).await.unwrap();
    assert!(view.has_html && view.text.contains("World"));
}

#[quench_test::test]
async fn nobody_can_be_written_to_with_no_recipients_or_too_many() {
    db!();
    let env = env().await;
    let author = Author {
        name: String::new(),
        address: "ann@home.example".into(),
        source: String::new(),
        owner: Some("ann".into()),
    };
    let e = delivery::deliver(
        &env.app,
        &author,
        &Letter {
            subject: "s".into(),
            text: "t".into(),
            ..Letter::default()
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(e, Error::Invalid(_)));
}
