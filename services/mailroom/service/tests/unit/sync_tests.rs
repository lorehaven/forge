use crate::support::*;
use mailroom_service::mailbox::{self, ListSpec};
use mailroom_service::sync;

macro_rules! db {
    () => {
        if !have_database() {
            return;
        }
    };
}

fn spec(folder: &str) -> ListSpec {
    ListSpec {
        folder_id: Some(folder.to_string()),
        limit: 50,
        ..ListSpec::default()
    }
}

#[quench_test::test]
async fn the_first_look_caches_folders_and_the_newest_messages() {
    db!();
    let env = env_with(|c| c.initial_messages = 3).await;
    for i in 1..=5 {
        env.deliver("INBOX", &format!("mail {i}"), "body");
    }
    let account = env.add_account(OWNER).await;
    let folders = mailbox::folders(&env.app, OWNER, &account.id)
        .await
        .unwrap();
    let roles: Vec<&str> = folders.iter().map(|f| f.role.as_str()).collect();
    assert_eq!(roles, ["inbox", "sent", "archive", "trash"]);
    let inbox = env.folder(&account.id, "inbox").await;
    assert_eq!((inbox.total, inbox.unread), (3, 3));
    let list = mailbox::list(&env.app, OWNER, &spec(&inbox.id))
        .await
        .unwrap();
    assert_eq!(list.total, 3);
    // Newest first, and only the newest three.
    let subjects: Vec<_> = list.items.iter().map(|m| m.subject.as_str()).collect();
    assert_eq!(subjects, ["mail 5", "mail 4", "mail 3"]);
    assert_eq!(list.items[0].from.name, "Alice");
    assert_eq!(list.items[0].from.address, "alice@x.example");
    assert_eq!(list.items[0].to[0].address, "me@example.com");
    // The account says it is fine and how many are unread.
    let account = mailroom_service::accounts::list(&env.app, OWNER)
        .await
        .unwrap()
        .remove(0);
    assert_eq!((account.state.as_str(), account.unread), ("ok", 3));
    assert!(account.last_sync.is_some() && account.last_error.is_none());
}

#[quench_test::test]
async fn later_runs_pick_up_new_removed_and_changed_messages() {
    db!();
    let env = env().await;
    let a = env.deliver("INBOX", "first", "x");
    let b = env.deliver("INBOX", "second", "x");
    let account = env.add_account(OWNER).await;
    let inbox = env.folder(&account.id, "inbox").await;
    assert_eq!(inbox.total, 2);

    env.deliver("INBOX", "third", "x");
    env.imap.expunge_uid("me@example.com", "INBOX", a);
    env.imap
        .set_flags("me@example.com", "INBOX", b, &["\\Seen", "\\Flagged"]);
    sync::sync_account(&env.app, &account.id).await.unwrap();

    let list = mailbox::list(&env.app, OWNER, &spec(&inbox.id))
        .await
        .unwrap();
    let subjects: Vec<_> = list.items.iter().map(|m| m.subject.as_str()).collect();
    assert_eq!(subjects, ["third", "second"]);
    let second = list.items.iter().find(|m| m.subject == "second").unwrap();
    assert!(second.seen && second.flagged && !second.answered);
    let inbox = env.folder(&account.id, "inbox").await;
    assert_eq!((inbox.total, inbox.unread), (2, 1));
}

#[quench_test::test]
async fn a_renumbered_folder_is_fetched_again_not_trusted() {
    db!();
    let env = env().await;
    env.deliver("INBOX", "old one", "x");
    let account = env.add_account(OWNER).await;
    env.imap.reset_folder("me@example.com", "INBOX");
    env.deliver("INBOX", "fresh one", "x");
    sync::sync_account(&env.app, &account.id).await.unwrap();
    let inbox = env.folder(&account.id, "inbox").await;
    let list = mailbox::list(&env.app, OWNER, &spec(&inbox.id))
        .await
        .unwrap();
    assert_eq!(
        list.items
            .iter()
            .map(|m| m.subject.as_str())
            .collect::<Vec<_>>(),
        ["fresh one"]
    );
}

#[quench_test::test]
async fn the_inbox_gets_previews_from_a_few_bodies() {
    db!();
    let env = env().await;
    env.deliver(
        "INBOX",
        "preview me",
        "Hello   there,\r\nthis is the   start of the message.",
    );
    let account = env.add_account(OWNER).await;
    let inbox = env.folder(&account.id, "inbox").await;
    let list = mailbox::list(&env.app, OWNER, &spec(&inbox.id))
        .await
        .unwrap();
    assert_eq!(
        list.items[0].preview,
        "Hello there, this is the start of the message."
    );
}

#[quench_test::test]
async fn a_failing_mailbox_records_what_went_wrong_in_words() {
    db!();
    let env = env().await;
    let account = env.add_account(OWNER).await;
    // The password was changed at the provider.
    env.imap.add_account("me@example.com", "changed-elsewhere");
    sync::sync_account(&env.app, &account.id).await.unwrap();
    let account = mailroom_service::accounts::list(&env.app, OWNER)
        .await
        .unwrap()
        .remove(0);
    assert_eq!(account.state, "error");
    let error = account.last_error.unwrap();
    assert!(error.contains("refused the login") && !error.contains(PASSWORD));
    // Fixed again: the next run clears it.
    env.imap.add_account("me@example.com", PASSWORD);
    sync::sync_account(&env.app, &account.id).await.unwrap();
    let account = mailroom_service::accounts::list(&env.app, OWNER)
        .await
        .unwrap()
        .remove(0);
    assert_eq!((account.state.as_str(), account.last_error), ("ok", None));
}

#[quench_test::test]
async fn a_disabled_mailbox_is_not_read_and_a_removed_one_is_skipped() {
    db!();
    let env = env().await;
    let account = env.add_account(OWNER).await;
    mailroom_service::admin::set_account_disabled(&env.app, "root", &account.id, true)
        .await
        .unwrap();
    let logins = env.imap.logins();
    sync::sync_account(&env.app, &account.id).await.unwrap();
    assert_eq!(env.imap.logins(), logins);
    assert_eq!(
        mailroom_service::accounts::list(&env.app, OWNER)
            .await
            .unwrap()[0]
            .state,
        "disabled"
    );
    sync::sync_account(&env.app, "no-such-account")
        .await
        .unwrap();
}

#[quench_test::test]
async fn searching_and_paging_and_the_unified_inbox() {
    db!();
    let env = env().await;
    for i in 0..7 {
        env.deliver("INBOX", &format!("invoice {i}"), "pay");
    }
    env.imap.deliver(
        "me@example.com",
        "INBOX",
        &raw("lunch plans", "Bob <bob@x.example>", "noon"),
        &["\\Seen"],
    );
    let account = env.add_account(OWNER).await;
    let inbox = env.folder(&account.id, "inbox").await;
    let page = |offset, limit| ListSpec {
        folder_id: Some(inbox.id.clone()),
        offset,
        limit,
        ..ListSpec::default()
    };
    let first = mailbox::list(&env.app, OWNER, &page(0, 3)).await.unwrap();
    let second = mailbox::list(&env.app, OWNER, &page(3, 3)).await.unwrap();
    assert_eq!(
        (first.items.len(), second.items.len(), first.total),
        (3, 3, 8)
    );
    assert!(
        first
            .items
            .iter()
            .all(|a| second.items.iter().all(|b| a.id != b.id))
    );

    let search = |text: &str| ListSpec {
        folder_id: Some(inbox.id.clone()),
        text: text.into(),
        limit: 50,
        ..ListSpec::default()
    };
    assert_eq!(
        mailbox::list(&env.app, OWNER, &search("invoice"))
            .await
            .unwrap()
            .total,
        7
    );
    assert_eq!(
        mailbox::list(&env.app, OWNER, &search("INVOICE 3"))
            .await
            .unwrap()
            .total,
        1
    );
    assert_eq!(
        mailbox::list(&env.app, OWNER, &search("bob"))
            .await
            .unwrap()
            .total,
        1
    );
    assert_eq!(
        mailbox::list(&env.app, OWNER, &search("bob@x.example"))
            .await
            .unwrap()
            .total,
        1
    );
    assert_eq!(
        mailbox::list(&env.app, OWNER, &search("nothing"))
            .await
            .unwrap()
            .total,
        0
    );
    // LIKE's own characters are searched for, not interpreted.
    assert_eq!(
        mailbox::list(&env.app, OWNER, &search("%"))
            .await
            .unwrap()
            .total,
        0
    );
    assert_eq!(
        mailbox::list(&env.app, OWNER, &search("_"))
            .await
            .unwrap()
            .total,
        0
    );
    assert_eq!(
        mailbox::list(&env.app, OWNER, &search("\\"))
            .await
            .unwrap()
            .total,
        0
    );
    assert_eq!(
        mailbox::list(&env.app, OWNER, &search("' OR 1=1 --"))
            .await
            .unwrap()
            .total,
        0
    );

    let unread = ListSpec {
        folder_id: Some(inbox.id.clone()),
        unread: true,
        limit: 50,
        ..ListSpec::default()
    };
    assert_eq!(
        mailbox::list(&env.app, OWNER, &unread).await.unwrap().total,
        7
    );
    let starred = ListSpec {
        folder_id: Some(inbox.id.clone()),
        flagged: true,
        limit: 50,
        ..ListSpec::default()
    };
    assert_eq!(
        mailbox::list(&env.app, OWNER, &starred)
            .await
            .unwrap()
            .total,
        0
    );

    // The same mail across mailboxes, by role.
    let again = env.add_account(OWNER).await;
    let unified = ListSpec {
        role: Some("inbox".into()),
        limit: 100,
        ..ListSpec::default()
    };
    let both = mailbox::list(&env.app, OWNER, &unified).await.unwrap();
    assert_eq!(both.total, 16);
    assert!(both.items.iter().any(|m| m.account_id == again.id));
    assert_eq!(
        mailbox::list(&env.app, "bob", &unified)
            .await
            .unwrap()
            .total,
        0
    );
}

#[quench_test::test]
async fn lists_are_the_owners_only_and_bounded() {
    db!();
    let env = env().await;
    env.deliver("INBOX", "private", "x");
    let account = env.add_account(OWNER).await;
    let inbox = env.folder(&account.id, "inbox").await;
    let mine = spec(&inbox.id);
    assert!(matches!(
        mailbox::list(&env.app, "bob", &mine).await,
        Err(mailroom_service::error::Error::NotFound)
    ));
    assert!(matches!(
        mailbox::list(&env.app, OWNER, &spec("no-such-folder")).await,
        Err(mailroom_service::error::Error::NotFound)
    ));
    assert!(matches!(
        mailbox::list(&env.app, OWNER, &ListSpec::default()).await,
        Err(mailroom_service::error::Error::Invalid(_))
    ));
    let odd_role = ListSpec {
        role: Some("all".into()),
        ..ListSpec::default()
    };
    assert!(matches!(
        mailbox::list(&env.app, OWNER, &odd_role).await,
        Err(mailroom_service::error::Error::Invalid(_))
    ));
    let huge = ListSpec {
        folder_id: Some(inbox.id),
        limit: 100_000,
        ..ListSpec::default()
    };
    assert!(
        mailbox::list(&env.app, OWNER, &huge)
            .await
            .unwrap()
            .items
            .len()
            <= 100
    );
}

#[quench_test::test]
async fn refresh_asks_the_server_now_and_is_rate_limited() {
    db!();
    let env = env().await;
    let account = env.add_account(OWNER).await;
    env.deliver("INBOX", "just arrived", "x");
    mailbox::refresh(&env.app, OWNER, &account.id)
        .await
        .unwrap();
    let inbox = env.folder(&account.id, "inbox").await;
    assert_eq!(inbox.total, 1);
    for _ in 0..19 {
        let _ = mailbox::refresh(&env.app, OWNER, &account.id).await;
    }
    assert!(matches!(
        mailbox::refresh(&env.app, OWNER, &account.id).await,
        Err(mailroom_service::error::Error::Limited(_))
    ));
}

#[quench_test::test]
async fn the_scheduler_syncs_what_is_due_and_what_was_asked_for() {
    db!();
    let env = env().await;
    let account = env.add_account(OWNER).await;
    env.deliver("INBOX", "scheduled", "x");
    let slots = std::sync::Arc::new(tokio::sync::Semaphore::new(2));
    // Adding the mailbox asked for a look; that was the one just made.
    env.app.hub.take_requested();
    // Just synced: not due, nothing happens.
    mailroom_service::worker::sync_due(&env.app, &slots)
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    assert_eq!(env.folder(&account.id, "inbox").await.total, 0);
    // Asked for: synced now.
    env.app.hub.request(&account.id);
    mailroom_service::worker::sync_due(&env.app, &slots)
        .await
        .unwrap();
    for _ in 0..50 {
        if env.folder(&account.id, "inbox").await.total == 1 {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    panic!("the requested sync did not run");
}
