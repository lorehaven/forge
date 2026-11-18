//! The panel: settings, mailboxes (never their mail), the services' deliveries, the audit log, and the sweep.

use crate::support::*;
use mailroom_core::admin::SettingsInput;
use mailroom_service::admin;
use mailroom_service::error::Error;
use mailroom_service::worker::{self, DELIVERY_DAYS, Swept, TRASH_DAYS};
use mailroom_service::{mailbox, outgoing};

macro_rules! db {
    () => {
        if !have_database() {
            return;
        }
    };
}

#[quench_test::test]
async fn settings_change_by_part_are_checked_and_audited() {
    db!();
    let env = env().await;
    let shown = admin::settings(&env.app).unwrap();
    assert_eq!(shown.domain, "home.example");
    assert_eq!((shown.quota_mb, shown.hourly_per_person), (500, 120));
    let changed = admin::update_settings(
        &env.app,
        "adam",
        &SettingsInput {
            quota_mb: Some(50),
            daily_budget: Some(10),
            ..SettingsInput::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(
        (
            changed.quota_mb,
            changed.daily_budget,
            changed.hourly_per_source
        ),
        (50, 10, 600)
    );
    // It is stored: a fresh start reads it back.
    assert_eq!(
        mailroom_service::settings::load(&env.app.store)
            .await
            .unwrap()
            .quota_mb,
        50
    );
    for bad in [
        SettingsInput {
            quota_mb: Some(0),
            ..SettingsInput::default()
        },
        SettingsInput {
            quota_mb: Some(100_001),
            ..SettingsInput::default()
        },
        SettingsInput {
            daily_budget: Some(0),
            ..SettingsInput::default()
        },
        SettingsInput {
            hourly_per_source: Some(100_001),
            ..SettingsInput::default()
        },
        SettingsInput {
            hourly_per_person: Some(10_001),
            ..SettingsInput::default()
        },
    ] {
        assert!(matches!(
            admin::update_settings(&env.app, "adam", &bad)
                .await
                .unwrap_err(),
            Error::Invalid(_)
        ));
    }
    assert_eq!(
        env.app.settings().quota_mb,
        50,
        "a bad form changes nothing"
    );
    admin::update_settings(
        &env.app,
        "adam",
        &SettingsInput {
            hourly_per_source: Some(7),
            hourly_per_person: Some(8),
            ..SettingsInput::default()
        },
    )
    .await
    .unwrap();
    let audit = admin::audit(&env.app, 0, 10).await.unwrap();
    assert_eq!(audit.total, 2);
    assert_eq!(
        (
            audit.items[0].actor.as_str(),
            audit.items[0].action.as_str()
        ),
        ("adam", "settings")
    );
    assert!(audit.items[1].detail.contains("mailbox size"));
    // Concurrent edits do not undo each other.
    let a = env.app.clone();
    let b = env.app.clone();
    let first = SettingsInput {
        daily_budget: Some(11),
        ..SettingsInput::default()
    };
    let second = SettingsInput {
        quota_mb: Some(12),
        ..SettingsInput::default()
    };
    let (x, y) = tokio::join!(
        admin::update_settings(&a, "adam", &first),
        admin::update_settings(&b, "eve", &second),
    );
    x.unwrap();
    y.unwrap();
    let now = env.app.settings();
    assert_eq!((now.daily_budget, now.quota_mb), (11, 12));
}

#[quench_test::test]
async fn the_panel_lists_mailboxes_but_never_their_mail() {
    db!();
    let env = env().await;
    env.people(&["ann", "bob", "cy"]).await;
    env.write("ann", &["bob"], "secret subject", "secret body")
        .await;
    let page = admin::mailboxes(&env.app, 0, 10).await.unwrap();
    assert_eq!(page.total, 3);
    let bob = page.items.iter().find(|m| m.owner == "bob").unwrap();
    assert_eq!(
        (bob.address.as_str(), bob.messages),
        ("bob@home.example", 1)
    );
    assert!(bob.bytes > 0 && bob.last_seen.is_some() && !bob.disabled);
    let ann = page.items.iter().find(|m| m.owner == "ann").unwrap();
    assert_eq!(ann.messages, 1, "the copy in sent");
    let json = quench_serde::json::to_string(&page);
    assert!(!json.contains("secret"), "{json}");
    // Paging.
    let second = admin::mailboxes(&env.app, 2, 10).await.unwrap();
    assert_eq!((second.items.len(), second.total), (1, 3));
    assert_eq!(
        admin::mailboxes(&env.app, -5, 0).await.unwrap().items.len(),
        1,
        "a nonsense page is clamped"
    );
    // Disable, enable, delete.
    admin::set_disabled(&env.app, "adam", "bob", true)
        .await
        .unwrap();
    assert!(
        admin::mailboxes(&env.app, 0, 10)
            .await
            .unwrap()
            .items
            .iter()
            .any(|m| m.owner == "bob" && m.disabled)
    );
    assert!(matches!(
        mailbox::open_mailbox(&env.app, "bob").await.unwrap_err(),
        Error::Forbidden
    ));
    admin::set_disabled(&env.app, "adam", "bob", false)
        .await
        .unwrap();
    mailbox::open_mailbox(&env.app, "bob").await.unwrap();
    outgoing::upload(&env.app, "bob", "f", "a/b", vec![1])
        .await
        .unwrap();
    admin::delete_mailbox(&env.app, "adam", "bob")
        .await
        .unwrap();
    assert_eq!(admin::mailboxes(&env.app, 0, 10).await.unwrap().total, 2);
    assert_eq!(
        env.app.store.uploads_bytes_of("bob").await.unwrap(),
        0,
        "their files went with it"
    );
    assert!(matches!(
        admin::delete_mailbox(&env.app, "adam", "bob")
            .await
            .unwrap_err(),
        Error::NotFound
    ));
    assert!(matches!(
        admin::set_disabled(&env.app, "adam", "bob", true)
            .await
            .unwrap_err(),
        Error::NotFound
    ));
    // A deleted name starts clean.
    assert!(env.folder("bob", "inbox").await.is_empty());
    let actions: Vec<_> = admin::audit(&env.app, 0, 10)
        .await
        .unwrap()
        .items
        .into_iter()
        .map(|a| a.action)
        .collect();
    assert_eq!(actions, ["delete-mailbox", "enable", "disable"]);
}

#[quench_test::test]
async fn the_delivery_log_shows_what_services_did_and_not_what_people_wrote() {
    db!();
    let env = env().await;
    env.people(&["ann", "bob"]).await;
    env.write("ann", &["bob"], "personal matter", "t").await;
    env.service_mail("gatehouse-mailroom", &["ann", "bob"], "Your link", "t")
        .await
        .unwrap();
    let log = admin::deliveries(&env.app, 0, 10).await.unwrap();
    assert_eq!(log.total, 1);
    let d = &log.items[0];
    assert_eq!(
        (
            d.source.as_str(),
            d.subject.as_str(),
            d.status.as_str(),
            d.delivered
        ),
        ("gatehouse", "Your link", "delivered", 2)
    );
    assert_eq!(d.recipients, ["ann@home.example", "bob@home.example"]);
    assert!(!quench_serde::json::to_string(&log).contains("personal matter"));
    let stats = admin::stats(&env.app).await.unwrap();
    assert_eq!(
        (
            stats.mailboxes,
            stats.messages,
            stats.delivered_24h,
            stats.failed_24h
        ),
        (2, 4, 2, 0)
    );
    assert_eq!(stats.budget_left, 2000 - 2);
    assert!(stats.bytes > 0);
}

#[quench_test::test]
async fn the_sweep_removes_only_what_is_old_enough() {
    db!();
    let env = env().await;
    env.people(&["ann", "bob"]).await;
    env.write("ann", &["bob"], "old trash", "t").await;
    env.write("ann", &["bob"], "new trash", "t").await;
    let up = outgoing::upload(&env.app, "ann", "u", "a/b", vec![1])
        .await
        .unwrap();
    env.service_mail("gatehouse", &["ann"], "s", "t")
        .await
        .unwrap();
    let ids: Vec<String> = env
        .folder("bob", "inbox")
        .await
        .iter()
        .map(|m| m.id.clone())
        .collect();
    let toss = mailroom_core::Change {
        ids: ids.clone(),
        move_to: Some("trash".into()),
        ..Default::default()
    };
    mailbox::change(&env.app, "bob", &toss).await.unwrap();
    let now = mailroom_service::error::now();
    // Backdate one message in the trash.
    env.app
        .store
        .pool
        .execute(
            "UPDATE messages SET moved = $1 WHERE id = $2",
            &[&(now - (TRASH_DAYS + 1) * 86_400), &ids[0]],
        )
        .await
        .unwrap();
    env.app
        .store
        .pool
        .execute(
            "UPDATE deliveries SET created = $1",
            &[&(now - (DELIVERY_DAYS + 1) * 86_400)],
        )
        .await
        .unwrap();
    // A blob nobody points at, old and young.
    env.app
        .store
        .pool
        .execute(
            "INSERT INTO blobs (id, data, created) VALUES ('old', 'x', $1), ('young', 'x', $2)",
            &[&(now - 7200), &now],
        )
        .await
        .unwrap();
    let swept = worker::sweep(&env.app, now).await.unwrap();
    assert_eq!(
        swept,
        Swept {
            uploads: 0,
            trash: 1,
            blobs: 1,
            deliveries: 1
        }
    );
    assert_eq!(env.folder("bob", "trash").await.len(), 1);
    assert_eq!(
        env.app
            .store
            .uploads_owned("ann", &[up.id.clone()])
            .await
            .unwrap()
            .len(),
        1,
        "a fresh upload stays"
    );
    // Tomorrow, the unsent upload goes too.
    let later = worker::sweep(&env.app, now + 2 * 86_400).await.unwrap();
    assert_eq!(later.uploads, 1);
    assert_eq!(
        worker::sweep(&env.app, now + 2 * 86_400).await.unwrap(),
        Swept::default()
    );
}
