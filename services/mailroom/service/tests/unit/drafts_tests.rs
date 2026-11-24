//! Drafts and muted senders.

use crate::support::*;
use mailroom_core::{Compose, DraftInput};
use mailroom_service::drafts;
use mailroom_service::error::Error;
use mailroom_service::mailbox;
use mailroom_service::outgoing;

macro_rules! db {
    () => {
        if !have_database() {
            return;
        }
    };
}

fn input(to: &str, subject: &str, text: &str) -> DraftInput {
    DraftInput {
        to: to.into(),
        subject: subject.into(),
        text: text.into(),
        ..DraftInput::default()
    }
}

#[quench_test::test]
async fn a_draft_is_kept_changed_listed_and_thrown_away_by_its_owner_only() {
    db!();
    let env = env().await;
    env.people(&["ann", "bob"]).await;
    // Nothing is checked yet: half an address and no subject are fine.
    let made = drafts::save(&env.app, "ann", None, &input("bo", "", "dear"))
        .await
        .unwrap();
    assert_eq!(made.to, "bo");
    let again = drafts::save(
        &env.app,
        "ann",
        Some(&made.id),
        &input("bob", "Hi", "dear bob"),
    )
    .await
    .unwrap();
    assert_eq!(again.id, made.id, "the same draft, changed");
    let list = drafts::list(&env.app, "ann").await.unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(
        (
            list[0].to.as_str(),
            list[0].subject.as_str(),
            list[0].text.as_str()
        ),
        ("bob", "Hi", "dear bob")
    );
    // The count is among the folders.
    let folders = mailbox::folders(&env.app, "ann").await.unwrap();
    assert_eq!(folders.iter().find(|f| f.id == "drafts").unwrap().total, 1);
    // Someone else cannot read, change or delete it.
    assert!(matches!(
        drafts::get(&env.app, "bob", &made.id).await.unwrap_err(),
        Error::NotFound
    ));
    assert!(matches!(
        drafts::save(&env.app, "bob", Some(&made.id), &input("ann", "x", "y"))
            .await
            .unwrap_err(),
        Error::NotFound
    ));
    drafts::delete(&env.app, "bob", &made.id).await.unwrap();
    assert_eq!(
        drafts::list(&env.app, "ann").await.unwrap().len(),
        1,
        "bob's delete did nothing"
    );
    assert_eq!(
        drafts::get(&env.app, "ann", &made.id)
            .await
            .unwrap()
            .subject,
        "Hi"
    );
    // A line break in the subject is a space; too long a draft is refused.
    let tidy = drafts::save(
        &env.app,
        "ann",
        Some(&made.id),
        &input("bob", "a\r\nb", "t"),
    )
    .await
    .unwrap();
    assert_eq!(tidy.subject, "a  b");
    let huge = input("bob", "x", &"y".repeat(drafts::MAX_TEXT + 1));
    assert!(matches!(
        drafts::save(&env.app, "ann", None, &huge)
            .await
            .unwrap_err(),
        Error::Invalid(_)
    ));
    drafts::delete(&env.app, "ann", &made.id).await.unwrap();
    assert!(drafts::list(&env.app, "ann").await.unwrap().is_empty());
    // Deleting what is gone is not an error.
    drafts::delete(&env.app, "ann", &made.id).await.unwrap();
}

#[quench_test::test]
async fn one_person_keeps_a_limited_number_of_drafts() {
    db!();
    let env = env().await;
    env.people(&["ann"]).await;
    for _ in 0..drafts::MAX_DRAFTS {
        drafts::save(&env.app, "ann", None, &input("a", "s", "t"))
            .await
            .unwrap();
    }
    assert!(matches!(
        drafts::save(&env.app, "ann", None, &input("a", "s", "t"))
            .await
            .unwrap_err(),
        Error::Invalid(_)
    ));
}

#[quench_test::test]
async fn sending_what_was_written_in_a_draft_removes_the_draft() {
    db!();
    let env = env().await;
    env.people(&["ann", "bob"]).await;
    let made = drafts::save(&env.app, "ann", None, &input("bob", "Hello", "hi"))
        .await
        .unwrap();
    outgoing::send_user(
        &env.app,
        "ann",
        &Compose {
            to: vec!["bob".into()],
            subject: "Hello".into(),
            text: "hi".into(),
            draft: Some(made.id.clone()),
            ..Compose::default()
        },
    )
    .await
    .unwrap();
    assert!(drafts::list(&env.app, "ann").await.unwrap().is_empty());
    assert_eq!(env.folder("bob", "inbox").await.len(), 1);
}

#[quench_test::test]
async fn a_muted_sender_goes_to_the_archive_read_until_let_back_in() {
    db!();
    let env = env().await;
    env.people(&["ann", "bob", "cy"]).await;
    drafts::mute(&env.app, "ann", "Bob@Home.Example")
        .await
        .unwrap();
    assert_eq!(
        drafts::mutes(&env.app, "ann").await.unwrap(),
        ["bob@home.example"]
    );
    env.write("bob", &["ann", "cy"], "muted", "t").await;
    assert!(env.folder("ann", "inbox").await.is_empty());
    let archived = env.folder("ann", "archive").await;
    assert_eq!(archived.len(), 1);
    assert!(archived[0].seen, "put away read");
    // It is only ann's choice: cy still hears from bob.
    assert_eq!(env.folder("cy", "inbox").await.len(), 1);
    drafts::unmute(&env.app, "ann", "bob@home.example")
        .await
        .unwrap();
    env.write("bob", &["ann"], "welcome back", "t").await;
    assert_eq!(env.folder("ann", "inbox").await.len(), 1);
    assert!(drafts::mutes(&env.app, "ann").await.unwrap().is_empty());
    for bad in ["", "a b"] {
        assert!(matches!(
            drafts::mute(&env.app, "ann", bad).await.unwrap_err(),
            Error::Invalid(_)
        ));
    }
}

#[quench_test::test]
async fn a_signature_is_kept_per_person_and_shown_with_who_they_are() {
    db!();
    let env = env().await;
    env.people(&["ann", "bob"]).await;
    assert_eq!(drafts::signature(&env.app, "ann").await.unwrap(), "");
    let kept = drafts::set_signature(&env.app, "ann", "Ann\nestate office\n\n")
        .await
        .unwrap();
    assert_eq!(kept, "Ann\nestate office", "trailing blank lines go");
    let me = mailbox::me(&env.app, "ann", true, false).await.unwrap();
    assert_eq!(me.signature, "Ann\nestate office");
    assert_eq!(
        mailbox::me(&env.app, "bob", true, false)
            .await
            .unwrap()
            .signature,
        ""
    );
    drafts::set_signature(&env.app, "ann", "").await.unwrap();
    assert_eq!(drafts::signature(&env.app, "ann").await.unwrap(), "");
    let long = "x".repeat(drafts::MAX_SIGNATURE + 1);
    assert!(matches!(
        drafts::set_signature(&env.app, "ann", &long)
            .await
            .unwrap_err(),
        Error::Invalid(_)
    ));
}
