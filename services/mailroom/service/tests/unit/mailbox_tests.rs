//! Reading: folders, lists, search, opening, changes.

use crate::support::*;
use mailroom_core::{Change, SendRequest};
use mailroom_service::error::Error;
use mailroom_service::mailbox::{self, ListSpec};
use mailroom_service::outgoing;

macro_rules! db {
    () => {
        if !have_database() {
            return;
        }
    };
}

fn spec(folder: &str) -> ListSpec {
    ListSpec {
        folder: folder.into(),
        limit: 50,
        ..ListSpec::default()
    }
}

#[quench_test::test]
async fn the_mailbox_is_made_on_first_use_with_three_folders() {
    db!();
    let env = env().await;
    let me = mailbox::me(&env.app, "Ann", true, false).await;
    assert!(
        me.is_err(),
        "user names are lower-cased by the caller; an upper-case one is not a mailbox name here"
    );
    let me = mailbox::me(&env.app, "ann", true, false).await.unwrap();
    assert_eq!(
        (me.address.as_str(), me.can_write, me.admin),
        ("ann@home.example", true, false)
    );
    let folders = mailbox::folders(&env.app, "ann").await.unwrap();
    let ids: Vec<_> = folders.iter().map(|f| f.id.as_str()).collect();
    assert_eq!(ids, ["inbox", "sent", "trash"]);
    assert!(folders.iter().all(|f| f.unread == 0 && f.total == 0));
    // Names that cannot be addresses have no mailbox.
    for bad in ["", "a b", "a@b", "../x"] {
        assert!(
            matches!(
                mailbox::open_mailbox(&env.app, bad).await.unwrap_err(),
                Error::Invalid(_)
            ),
            "{bad}"
        );
    }
}

#[quench_test::test]
async fn counts_lists_search_and_paging() {
    db!();
    let env = env().await;
    env.people(&["ann", "bob"]).await;
    for (i, word) in ["alpha", "beta", "gamma", "delta"].iter().enumerate() {
        env.write(
            "bob",
            &["ann"],
            &format!("{word} report"),
            &format!("body number {i} about {word}"),
        )
        .await;
    }
    let folders = mailbox::folders(&env.app, "ann").await.unwrap();
    assert_eq!((folders[0].unread, folders[0].total), (4, 4));
    let all = mailbox::list(&env.app, "ann", &spec("inbox"))
        .await
        .unwrap();
    assert_eq!(all.total, 4);
    assert_eq!(all.items.len(), 4);
    // Search looks at the subject, the sender, the text; every word must be found.
    for (text, expect) in [
        ("beta", 1),
        ("REPORT", 4),
        ("bob@home.example", 4),
        ("number 2 gamma", 1),
        ("beta gamma", 0),
        ("zzz", 0),
        ("100%", 0),
        ("a_b", 0),
    ] {
        let found = mailbox::list(
            &env.app,
            "ann",
            &ListSpec {
                text: text.into(),
                ..spec("inbox")
            },
        )
        .await
        .unwrap();
        assert_eq!(found.total, expect, "{text}");
    }
    // Paging.
    let page = mailbox::list(
        &env.app,
        "ann",
        &ListSpec {
            limit: 3,
            offset: 3,
            ..spec("inbox")
        },
    )
    .await
    .unwrap();
    assert_eq!((page.items.len(), page.total, page.offset), (1, 4, 3));
    let capped = mailbox::list(
        &env.app,
        "ann",
        &ListSpec {
            limit: 100_000,
            ..spec("inbox")
        },
    )
    .await
    .unwrap();
    assert_eq!(capped.items.len(), 4);
    // Unread and starred filters.
    let first = all.items[0].id.clone();
    mailbox::change(
        &env.app,
        "ann",
        &Change {
            ids: vec![first.clone()],
            seen: Some(true),
            flagged: Some(true),
            ..Change::default()
        },
    )
    .await
    .unwrap();
    let unread = mailbox::list(
        &env.app,
        "ann",
        &ListSpec {
            unread: true,
            ..spec("inbox")
        },
    )
    .await
    .unwrap();
    assert_eq!(unread.total, 3);
    let starred = mailbox::list(
        &env.app,
        "ann",
        &ListSpec {
            flagged: true,
            ..spec("inbox")
        },
    )
    .await
    .unwrap();
    assert_eq!(
        (starred.total, starred.items[0].id.as_str()),
        (1, first.as_str())
    );
    // A folder that does not exist, and the default folder.
    assert!(matches!(
        mailbox::list(&env.app, "ann", &spec("spam"))
            .await
            .unwrap_err(),
        Error::Invalid(_)
    ));
    assert_eq!(
        mailbox::list(
            &env.app,
            "ann",
            &ListSpec {
                limit: 5,
                ..ListSpec::default()
            }
        )
        .await
        .unwrap()
        .total,
        4
    );
    // Sent holds bob's copies.
    assert_eq!(
        mailbox::list(&env.app, "bob", &spec("sent"))
            .await
            .unwrap()
            .total,
        4
    );
}

#[quench_test::test]
async fn a_message_is_opened_only_by_its_owner() {
    db!();
    let env = env().await;
    env.people(&["ann", "bob", "cy"]).await;
    env.write("ann", &["bob"], "private", "for bob only").await;
    let id = env.folder("bob", "inbox").await[0].id.clone();
    let view = mailbox::open(&env.app, "bob", &id).await.unwrap();
    assert_eq!(
        (view.summary.subject.as_str(), view.text.as_str()),
        ("private", "for bob only")
    );
    assert!(!view.has_html && view.attachments.is_empty() && view.remote_images == 0);
    assert!(view.message_id.starts_with('<') && view.message_id.ends_with("@home.example>"));
    for stranger in ["cy"] {
        assert!(matches!(
            mailbox::open(&env.app, stranger, &id).await.unwrap_err(),
            Error::NotFound
        ));
        assert!(matches!(
            mailbox::html(&env.app, stranger, &id, false)
                .await
                .unwrap_err(),
            Error::NotFound
        ));
        assert!(matches!(
            mailbox::attachment(&env.app, stranger, &id, 0)
                .await
                .unwrap_err(),
            Error::NotFound
        ));
        assert!(matches!(
            mailbox::change(
                &env.app,
                stranger,
                &Change {
                    ids: vec![id.clone()],
                    seen: Some(true),
                    ..Change::default()
                }
            )
            .await
            .unwrap_err(),
            Error::NotFound
        ));
    }
    // Not even the sender reaches the recipient's copy.
    assert!(matches!(
        mailbox::open(&env.app, "ann", &id).await.unwrap_err(),
        Error::NotFound
    ));
    assert!(matches!(
        mailbox::attachment(&env.app, "bob", &id, u32::MAX)
            .await
            .unwrap_err(),
        Error::NotFound
    ));
}

#[quench_test::test]
async fn html_is_sanitised_with_remote_images_held_back() {
    db!();
    let env = env().await;
    outgoing::send_service(
        &env.app,
        "gatehouse",
        &SendRequest {
            to: vec!["ann".into()],
            subject: "html".into(),
            text: "plain".into(),
            html: Some("<p onclick=\"x()\">hi</p><script>alert(1)</script><img src=\"https://t.example/p.gif\"><a href=\"javascript:alert(1)\">x</a>".into()),
            ..SendRequest::default()
        },
    )
    .await
    .unwrap();
    let id = env.folder("ann", "inbox").await[0].id.clone();
    let view = mailbox::open(&env.app, "ann", &id).await.unwrap();
    assert!(view.has_html);
    assert_eq!(view.remote_images, 1);
    let page = mailbox::html(&env.app, "ann", &id, false).await.unwrap();
    assert!(page.contains("<p>hi</p>"));
    for bad in ["<script", "onclick", "t.example", "javascript:"] {
        assert!(!page.contains(bad), "{bad}: {page}");
    }
    let remote = mailbox::html(&env.app, "ann", &id, true).await.unwrap();
    assert!(remote.contains("https://t.example/p.gif"));
    // A message without HTML has none to show.
    env.people(&["bob"]).await;
    env.write("bob", &["ann"], "text", "just text").await;
    let plain = env
        .folder("ann", "inbox")
        .await
        .into_iter()
        .find(|m| m.subject == "text")
        .unwrap();
    assert!(matches!(
        mailbox::html(&env.app, "ann", &plain.id, false)
            .await
            .unwrap_err(),
        Error::NotFound
    ));
}

#[quench_test::test]
async fn changes_move_between_inbox_and_trash_and_delete_only_from_the_trash() {
    db!();
    let env = env().await;
    env.people(&["ann", "bob"]).await;
    env.write("bob", &["ann"], "keep", "t").await;
    env.write("bob", &["ann"], "toss", "t").await;
    let ids: Vec<String> = env
        .folder("ann", "inbox")
        .await
        .iter()
        .map(|m| m.id.clone())
        .collect();
    let trash = |ids: &[String]| Change {
        ids: ids.to_vec(),
        move_to: Some("trash".into()),
        ..Change::default()
    };
    mailbox::change(&env.app, "ann", &trash(&ids[..1]))
        .await
        .unwrap();
    assert_eq!(env.folder("ann", "trash").await.len(), 1);
    assert_eq!(env.folder("ann", "inbox").await.len(), 1);
    // Deleting for good is for the trash only.
    let delete = |ids: &[String]| Change {
        ids: ids.to_vec(),
        delete: true,
        ..Change::default()
    };
    assert!(matches!(
        mailbox::change(&env.app, "ann", &delete(&ids[1..]))
            .await
            .unwrap_err(),
        Error::Invalid(_)
    ));
    // Back to the inbox, then gone.
    let back = Change {
        ids: ids[..1].to_vec(),
        move_to: Some("inbox".into()),
        ..Change::default()
    };
    mailbox::change(&env.app, "ann", &back).await.unwrap();
    assert_eq!(env.folder("ann", "inbox").await.len(), 2);
    mailbox::change(&env.app, "ann", &trash(&ids))
        .await
        .unwrap();
    mailbox::change(&env.app, "ann", &delete(&ids))
        .await
        .unwrap();
    assert!(env.folder("ann", "trash").await.is_empty());
    assert!(matches!(
        mailbox::open(&env.app, "ann", &ids[0]).await.unwrap_err(),
        Error::NotFound
    ));
    // Mail a person sent comes back from the trash to "sent", never to the inbox.
    let mine = env.folder("bob", "sent").await[0].id.clone();
    mailbox::change(&env.app, "bob", &trash(&[mine.clone()]))
        .await
        .unwrap();
    assert_eq!(env.folder("bob", "trash").await.len(), 1);
    mailbox::change(
        &env.app,
        "bob",
        &Change {
            ids: vec![mine],
            move_to: Some("inbox".into()),
            ..Change::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(
        (
            env.folder("bob", "sent").await.len(),
            env.folder("bob", "inbox").await.len()
        ),
        (2, 0)
    );
}

#[quench_test::test]
async fn changes_are_validated() {
    db!();
    let env = env().await;
    env.people(&["ann"]).await;
    env.service_mail("gatehouse", &["ann"], "s", "t")
        .await
        .unwrap();
    let id = env.folder("ann", "inbox").await[0].id.clone();
    let cases = [
        Change {
            ids: vec![],
            seen: Some(true),
            ..Change::default()
        },
        Change {
            ids: (0..201).map(|i| i.to_string()).collect(),
            seen: Some(true),
            ..Change::default()
        },
        Change {
            ids: vec![id.clone()],
            ..Change::default()
        },
        Change {
            ids: vec![id.clone()],
            delete: true,
            move_to: Some("trash".into()),
            ..Change::default()
        },
        Change {
            ids: vec![id.clone()],
            move_to: Some("sent".into()),
            ..Change::default()
        },
        Change {
            ids: vec![id.clone()],
            move_to: Some("../x".into()),
            ..Change::default()
        },
    ];
    for (i, c) in cases.iter().enumerate() {
        assert!(
            matches!(
                mailbox::change(&env.app, "ann", c).await.unwrap_err(),
                Error::Invalid(_)
            ),
            "case {i}"
        );
    }
    let unknown = Change {
        ids: vec!["nope".into()],
        seen: Some(true),
        ..Change::default()
    };
    assert!(matches!(
        mailbox::change(&env.app, "ann", &unknown)
            .await
            .unwrap_err(),
        Error::NotFound
    ));
    // Unread again.
    mailbox::change(
        &env.app,
        "ann",
        &Change {
            ids: vec![id.clone()],
            seen: Some(true),
            ..Change::default()
        },
    )
    .await
    .unwrap();
    mailbox::change(
        &env.app,
        "ann",
        &Change {
            ids: vec![id],
            seen: Some(false),
            ..Change::default()
        },
    )
    .await
    .unwrap();
    assert!(!env.folder("ann", "inbox").await[0].seen);
}

#[quench_test::test]
async fn people_are_found_by_the_start_of_their_name() {
    db!();
    let env = env().await;
    env.people(&["ann", "anna", "bob", "a_b"]).await;
    let found = |prefix: &'static str| {
        let env = &env;
        async move {
            mailbox::people(&env.app, "ann", prefix)
                .await
                .unwrap()
                .into_iter()
                .map(|c| c.address)
                .collect::<Vec<_>>()
        }
    };
    assert_eq!(found("an").await, ["ann@home.example", "anna@home.example"]);
    assert_eq!(found("B").await, ["bob@home.example"]);
    assert_eq!(
        found("a_").await,
        ["a_b@home.example"],
        "an underscore is not a wildcard"
    );
    assert!(found("%").await.is_empty());
    assert!(
        found("").await.is_empty(),
        "the whole directory is not handed out in one request"
    );
}
