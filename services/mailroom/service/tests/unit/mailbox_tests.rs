use crate::support::*;
use mailroom_core::Change;
use mailroom_service::error::Error;
use mailroom_service::mailbox;

macro_rules! db {
    () => {
        if !have_database() {
            return;
        }
    };
}

/// A message with HTML (script, remote image, inline image) and two attachments.
fn rich() -> Vec<u8> {
    let png = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";
    format!(
        "From: Alice <alice@x.example>\r\nTo: me@example.com\r\nCc: carol@x.example\r\nReply-To: replies@x.example\r\nSubject: =?UTF-8?B?V2l0YWo=?=\r\n\
Date: Tue, 29 Sep 2026 12:00:00 +0000\r\nMessage-ID: <rich@x.example>\r\nList-Unsubscribe: <https://x.example/u>\r\nMIME-Version: 1.0\r\n\
Content-Type: multipart/mixed; boundary=\"m\"\r\n\r\n--m\r\nContent-Type: multipart/related; boundary=\"r\"\r\n\r\n--r\r\n\
Content-Type: multipart/alternative; boundary=\"a\"\r\n\r\n--a\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nPlain version\r\n--a\r\n\
Content-Type: text/html; charset=utf-8\r\n\r\n<p onclick=\"steal()\">Hello <b>world</b></p><script>alert(1)</script>\
<img src=\"cid:logo\"><img src=\"https://tracker.example/p.gif\"><a href=\"javascript:alert(2)\">bad</a><a href=\"https://ok.example/\">ok</a>\r\n--a--\r\n\
--r\r\nContent-Type: image/png\r\nContent-ID: <logo>\r\nContent-Disposition: inline\r\nContent-Transfer-Encoding: base64\r\n\r\n{png}\r\n--r--\r\n\
--m\r\nContent-Type: application/pdf; name=\"report.pdf\"\r\nContent-Disposition: attachment; filename=\"report.pdf\"\r\nContent-Transfer-Encoding: base64\r\n\r\nJVBERi0xLjQK\r\n\
--m\r\nContent-Type: text/html; name=\"evil.html\"\r\nContent-Disposition: attachment; filename=\"../evil.html\"\r\n\r\n<script>alert(3)</script>\r\n--m--\r\n"
    )
    .into_bytes()
}

async fn change(env: &Env, c: Change) -> Result<(), Error> {
    mailbox::change(&env.app, OWNER, &c).await
}

async fn with_rich() -> (Env, mailroom_core::Account, mailroom_core::MessageSummary) {
    let env = env().await;
    env.imap.deliver("me@example.com", "INBOX", &rich(), &[]);
    let account = env.add_account(OWNER).await;
    let inbox = env.folder(&account.id, "inbox").await;
    let list = mailbox::list(
        &env.app,
        OWNER,
        &mailbox::ListSpec {
            folder_id: Some(inbox.id),
            limit: 10,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let message = list.items.into_iter().next().unwrap();
    (env, account, message)
}

#[quench_test::test]
async fn opening_a_message_gives_text_headers_attachments_and_counts_remote_images() {
    db!();
    let (env, _account, message) = with_rich().await;
    assert_eq!(message.subject, "Witaj");
    assert!(message.attachments);
    let view = mailbox::open(&env.app, OWNER, &message.id).await.unwrap();
    assert!(view.text.contains("Plain version"));
    assert_eq!(view.cc[0].address, "carol@x.example");
    assert_eq!(view.reply_to[0].address, "replies@x.example");
    assert_eq!(view.message_id, "<rich@x.example>");
    assert_eq!(view.list_unsubscribe, "<https://x.example/u>");
    assert!(view.has_html);
    assert_eq!(view.remote_images, 1);
    let names: Vec<_> = view
        .attachments
        .iter()
        .map(|a| (a.filename.as_str(), a.inline))
        .collect();
    assert!(names.contains(&("report.pdf", false)), "{names:?}");
    assert!(names.contains(&("evil.html", false)), "{names:?}");
}

#[quench_test::test]
async fn the_html_is_safe_and_images_follow_the_readers_choice() {
    db!();
    let (env, _account, message) = with_rich().await;
    let safe = mailbox::html(&env.app, OWNER, &message.id, false)
        .await
        .unwrap();
    for needle in ["<script", "onclick", "javascript:", "steal(", "alert("] {
        assert!(!safe.contains(needle), "{needle} in {safe}");
    }
    assert!(safe.contains("<b>world</b>") && safe.contains("https://ok.example/"));
    assert!(
        safe.contains("data:image/png;base64,"),
        "the inline image is part of the page"
    );
    assert!(!safe.contains("tracker.example"));
    let with_images = mailbox::html(&env.app, OWNER, &message.id, true)
        .await
        .unwrap();
    assert!(with_images.contains("https://tracker.example/p.gif"));
    assert!(with_images.starts_with("<!doctype html>"));
    assert!(with_images.contains("<base target=\"_blank\">") && with_images.contains("referrer"));
}

#[quench_test::test]
async fn a_message_without_html_has_none_to_show() {
    db!();
    let env = env().await;
    env.deliver("INBOX", "plain", "just text");
    let account = env.add_account(OWNER).await;
    let inbox = env.folder(&account.id, "inbox").await;
    let id = mailbox::list(
        &env.app,
        OWNER,
        &mailbox::ListSpec {
            folder_id: Some(inbox.id),
            limit: 5,
            ..Default::default()
        },
    )
    .await
    .unwrap()
    .items[0]
        .id
        .clone();
    let view = mailbox::open(&env.app, OWNER, &id).await.unwrap();
    assert!(!view.has_html && view.remote_images == 0 && view.attachments.is_empty());
    assert!(matches!(
        mailbox::html(&env.app, OWNER, &id, false).await,
        Err(Error::NotFound)
    ));
}

#[quench_test::test]
async fn attachments_download_with_their_own_bytes_and_a_clean_name() {
    db!();
    let (env, _account, message) = with_rich().await;
    let view = mailbox::open(&env.app, OWNER, &message.id).await.unwrap();
    let pdf = view
        .attachments
        .iter()
        .find(|a| a.filename == "report.pdf")
        .unwrap();
    let (name, data) = mailbox::attachment(&env.app, OWNER, &message.id, pdf.index)
        .await
        .unwrap();
    assert_eq!(name, "report.pdf");
    assert_eq!(data, b"%PDF-1.4\n");
    let evil = view
        .attachments
        .iter()
        .find(|a| a.filename == "evil.html")
        .unwrap();
    let (name, _) = mailbox::attachment(&env.app, OWNER, &message.id, evil.index)
        .await
        .unwrap();
    assert_eq!(name, "evil.html", "no path from the sender survives");
    assert!(matches!(
        mailbox::attachment(&env.app, OWNER, &message.id, 99).await,
        Err(Error::NotFound)
    ));
}

#[quench_test::test]
async fn nobody_else_can_open_a_message_or_its_parts() {
    db!();
    let (env, _account, message) = with_rich().await;
    assert!(matches!(
        mailbox::open(&env.app, "bob", &message.id).await,
        Err(Error::NotFound)
    ));
    assert!(matches!(
        mailbox::html(&env.app, "bob", &message.id, false).await,
        Err(Error::NotFound)
    ));
    assert!(matches!(
        mailbox::attachment(&env.app, "bob", &message.id, 0).await,
        Err(Error::NotFound)
    ));
    assert!(matches!(
        mailbox::open(&env.app, OWNER, "no-such-id").await,
        Err(Error::NotFound)
    ));
}

#[quench_test::test]
async fn a_body_is_fetched_once_and_then_read_from_the_cache() {
    db!();
    let (env, _account, message) = with_rich().await;
    mailbox::open(&env.app, OWNER, &message.id).await.unwrap();
    let connections = env.imap.connections();
    mailbox::open(&env.app, OWNER, &message.id).await.unwrap();
    mailbox::html(&env.app, OWNER, &message.id, false)
        .await
        .unwrap();
    assert_eq!(
        env.imap.connections(),
        connections,
        "opening again must not call the mail server"
    );
}

#[quench_test::test]
async fn a_message_that_is_gone_from_the_server_is_not_found_and_triggers_a_refresh() {
    db!();
    let env = env().await;
    let uid = env.imap.deliver(
        "me@example.com",
        "Archive",
        &raw("moved away", "a@x.example", "x"),
        &[],
    );
    let account = env.add_account(OWNER).await;
    let folder = env.folder(&account.id, "archive").await;
    let id = mailbox::list(
        &env.app,
        OWNER,
        &mailbox::ListSpec {
            folder_id: Some(folder.id),
            limit: 5,
            ..Default::default()
        },
    )
    .await
    .unwrap()
    .items[0]
        .id
        .clone();
    env.imap.expunge_uid("me@example.com", "Archive", uid);
    assert!(matches!(
        mailbox::open(&env.app, OWNER, &id).await,
        Err(Error::NotFound)
    ));
    assert!(!env.app.hub.take_requested().is_empty());
}

#[quench_test::test]
async fn read_and_star_are_changed_on_the_server_and_in_the_cache() {
    db!();
    let env = env().await;
    env.deliver("INBOX", "toggle", "x");
    let account = env.add_account(OWNER).await;
    let inbox = env.folder(&account.id, "inbox").await;
    let id = mailbox::list(
        &env.app,
        OWNER,
        &mailbox::ListSpec {
            folder_id: Some(inbox.id.clone()),
            limit: 5,
            ..Default::default()
        },
    )
    .await
    .unwrap()
    .items[0]
        .id
        .clone();
    mailbox::change(
        &env.app,
        OWNER,
        &Change {
            ids: vec![id.clone()],
            seen: Some(true),
            flagged: Some(true),
            ..Change::default()
        },
    )
    .await
    .unwrap();
    let flags = env.imap.messages("me@example.com", "INBOX")[0]
        .flags
        .clone();
    assert!(
        flags.contains(&"\\Seen".to_string()) && flags.contains(&"\\Flagged".to_string()),
        "{flags:?}"
    );
    let view = mailbox::open(&env.app, OWNER, &id).await.unwrap();
    assert!(view.summary.seen && view.summary.flagged);
    assert_eq!(env.folder(&account.id, "inbox").await.unread, 0);
    mailbox::change(
        &env.app,
        OWNER,
        &Change {
            ids: vec![id.clone()],
            seen: Some(false),
            flagged: Some(false),
            ..Change::default()
        },
    )
    .await
    .unwrap();
    assert!(
        env.imap.messages("me@example.com", "INBOX")[0]
            .flags
            .is_empty()
    );
    assert_eq!(env.folder(&account.id, "inbox").await.unread, 1);
}

#[quench_test::test]
async fn messages_move_to_trash_and_are_deleted_for_good_only_from_there() {
    db!();
    let env = env().await;
    env.deliver("INBOX", "doomed", "x");
    let account = env.add_account(OWNER).await;
    let inbox = env.folder(&account.id, "inbox").await;
    let list = |folder: String| async {
        mailbox::list(
            &env.app,
            OWNER,
            &mailbox::ListSpec {
                folder_id: Some(folder),
                limit: 10,
                ..Default::default()
            },
        )
        .await
        .unwrap()
    };
    let id = list(inbox.id.clone()).await.items[0].id.clone();
    // Deleting from the inbox is refused: it is a move to the trash.
    let Err(Error::Invalid(text)) = mailbox::change(
        &env.app,
        OWNER,
        &Change {
            ids: vec![id.clone()],
            delete: true,
            ..Change::default()
        },
    )
    .await
    else {
        panic!()
    };
    assert!(text.contains("trash"));
    mailbox::change(
        &env.app,
        OWNER,
        &Change {
            ids: vec![id],
            move_to: Some("trash".into()),
            ..Change::default()
        },
    )
    .await
    .unwrap();
    assert!(env.imap.messages("me@example.com", "INBOX").is_empty());
    assert_eq!(env.imap.messages("me@example.com", "Trash").len(), 1);
    assert!(list(inbox.id).await.items.is_empty());
    // The next look finds it in the trash; deleting there removes it for good.
    mailroom_service::sync::sync_account(&env.app, &account.id)
        .await
        .unwrap();
    let trash = env.folder(&account.id, "trash").await;
    let in_trash = list(trash.id).await.items[0].id.clone();
    mailbox::change(
        &env.app,
        OWNER,
        &Change {
            ids: vec![in_trash],
            delete: true,
            ..Change::default()
        },
    )
    .await
    .unwrap();
    assert!(env.imap.messages("me@example.com", "Trash").is_empty());
}

#[quench_test::test]
async fn changes_are_checked_before_anything_is_done() {
    db!();
    let env = env().await;
    env.deliver("INBOX", "x", "x");
    let account = env.add_account(OWNER).await;
    let inbox = env.folder(&account.id, "inbox").await;
    let id = mailbox::list(
        &env.app,
        OWNER,
        &mailbox::ListSpec {
            folder_id: Some(inbox.id.clone()),
            limit: 5,
            ..Default::default()
        },
    )
    .await
    .unwrap()
    .items[0]
        .id
        .clone();
    assert!(matches!(
        change(&env, Change::default()).await,
        Err(Error::Invalid(_))
    ));
    assert!(matches!(
        change(
            &env,
            Change {
                ids: vec![id.clone()],
                ..Change::default()
            }
        )
        .await,
        Err(Error::Invalid(_))
    ));
    assert!(matches!(
        change(
            &env,
            Change {
                ids: vec![id.clone()],
                delete: true,
                move_to: Some("trash".into()),
                ..Change::default()
            }
        )
        .await,
        Err(Error::Invalid(_))
    ));
    assert!(matches!(
        change(
            &env,
            Change {
                ids: vec!["nope".into()],
                seen: Some(true),
                ..Change::default()
            }
        )
        .await,
        Err(Error::NotFound)
    ));
    assert!(matches!(
        change(
            &env,
            Change {
                ids: vec![id.clone(); 201],
                seen: Some(true),
                ..Change::default()
            }
        )
        .await,
        Err(Error::Invalid(_))
    ));
    assert!(matches!(
        mailbox::change(
            &env.app,
            "bob",
            &Change {
                ids: vec![id.clone()],
                seen: Some(true),
                ..Change::default()
            }
        )
        .await,
        Err(Error::NotFound)
    ));
    // A role the mailbox does not have, the folder it is already in, and another mailbox's folder.
    assert!(matches!(
        change(
            &env,
            Change {
                ids: vec![id.clone()],
                move_to: Some("junk".into()),
                ..Change::default()
            }
        )
        .await,
        Err(Error::Invalid(_))
    ));
    assert!(matches!(
        change(
            &env,
            Change {
                ids: vec![id.clone()],
                move_to: Some(inbox.id.clone()),
                ..Change::default()
            }
        )
        .await,
        Err(Error::Invalid(_))
    ));
    assert!(matches!(
        change(
            &env,
            Change {
                ids: vec![id.clone()],
                move_to: Some("no-such-folder".into()),
                ..Change::default()
            }
        )
        .await,
        Err(Error::NotFound)
    ));
    let other = env.add_account(OWNER).await;
    let foreign = env.folder(&other.id, "archive").await;
    assert!(matches!(
        change(
            &env,
            Change {
                ids: vec![id],
                move_to: Some(foreign.id),
                ..Change::default()
            }
        )
        .await,
        Err(Error::Invalid(_))
    ));
    assert_eq!(env.imap.messages("me@example.com", "INBOX").len(), 1);
}

#[quench_test::test]
async fn a_disabled_mailbox_cannot_be_read_or_changed() {
    db!();
    let (env, account, message) = with_rich().await;
    mailroom_service::admin::set_account_disabled(&env.app, "root", &account.id, true)
        .await
        .unwrap();
    assert!(matches!(
        mailbox::open(&env.app, OWNER, &message.id).await,
        Err(Error::Forbidden)
    ));
    assert!(matches!(
        mailbox::change(
            &env.app,
            OWNER,
            &Change {
                ids: vec![message.id.clone()],
                seen: Some(true),
                ..Change::default()
            }
        )
        .await,
        Err(Error::Forbidden)
    ));
}

#[quench_test::test]
async fn huge_messages_are_not_opened() {
    db!();
    let env = env().await;
    env.deliver("INBOX", "big", &"x".repeat(1000));
    let account = env.add_account(OWNER).await;
    let inbox = env.folder(&account.id, "inbox").await;
    let id = mailbox::list(
        &env.app,
        OWNER,
        &mailbox::ListSpec {
            folder_id: Some(inbox.id),
            limit: 5,
            ..Default::default()
        },
    )
    .await
    .unwrap()
    .items[0]
        .id
        .clone();
    env.app
        .store
        .pool
        .execute("UPDATE messages SET size = 50000000 WHERE id = $1", &[&id])
        .await
        .unwrap();
    env.app
        .store
        .pool
        .execute("DELETE FROM bodies", &[])
        .await
        .unwrap();
    let Err(Error::Invalid(text)) = mailbox::open(&env.app, OWNER, &id).await else {
        panic!()
    };
    assert!(text.contains("too large"));
}
