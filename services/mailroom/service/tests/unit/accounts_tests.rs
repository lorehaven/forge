use crate::support::*;
use mailroom_service::accounts;
use mailroom_service::error::Error;

macro_rules! db {
    () => {
        if !have_database() {
            return;
        }
    };
}

#[quench_test::test]
async fn an_account_needs_a_working_login_and_is_kept_sealed() {
    db!();
    let env = env().await;
    let account = accounts::create(&env.app, OWNER, &env.input())
        .await
        .unwrap();
    assert_eq!(account.address, "me@example.com");
    assert_eq!(account.state, "ok");
    // The password is in the database only as a sealed value.
    let row = env
        .app
        .store
        .account_get(&account.id)
        .await
        .unwrap()
        .unwrap();
    assert!(row.secret.starts_with("v1.") && !row.secret.contains(PASSWORD));
    assert_eq!(env.imap.logins(), 1);
    // And never in what the API says.
    let json = quench_serde::json::to_string(&account);
    assert!(!json.contains(PASSWORD) && !json.contains("v1."));
}

#[quench_test::test]
async fn a_wrong_password_is_refused_with_a_helpful_sentence_and_nothing_is_kept() {
    db!();
    let env = env().await;
    let mut input = env.input();
    input.password = "wrong".into();
    let error = accounts::create(&env.app, OWNER, &input).await.unwrap_err();
    let Error::Upstream(text) = error else {
        panic!("{error:?}")
    };
    assert!(text.contains("app password") && !text.contains("wrong"));
    assert!(accounts::list(&env.app, OWNER).await.unwrap().is_empty());
}

#[quench_test::test]
async fn forms_are_checked_before_any_connection() {
    db!();
    let env = env().await;
    let cases: Vec<(Box<dyn Fn(&mut mailroom_core::AccountInput)>, &str)> = vec![
        (Box::new(|i| i.address = "nope".into()), "email address"),
        (Box::new(|i| i.label = String::new()), "name"),
        (Box::new(|i| i.label = "x".repeat(61)), "name"),
        (Box::new(|i| i.username = String::new()), "user name"),
        (Box::new(|i| i.password = String::new()), "password"),
        (Box::new(|i| i.password = "a\nb".into()), "password"),
        (Box::new(|i| i.imap_host = "bad host".into()), "server name"),
        (Box::new(|i| i.smtp_host = "http://x".into()), "server name"),
        (Box::new(|i| i.imap_port = 0), "port"),
        (Box::new(|i| i.smtp_security = "none".into()), "TLS"),
        (Box::new(|i| i.signature = "x".repeat(4001)), "signature"),
        (
            Box::new(|i| i.tls_roots_pem = "not a certificate".into()),
            "certificate",
        ),
        (
            Box::new(|i| i.tls_roots_pem = "x".repeat(30_000)),
            "certificate",
        ),
    ];
    for (mutate, needle) in cases {
        let mut input = env.input();
        mutate(&mut input);
        let Err(Error::Invalid(text)) = accounts::create(&env.app, OWNER, &input).await else {
            panic!("{needle}")
        };
        assert!(text.contains(needle), "{text} should mention {needle}");
    }
    assert_eq!(env.imap.connections(), 0);
}

#[quench_test::test]
async fn internal_hosts_are_refused_unless_an_administrator_allowed_them() {
    db!();
    let env = env_with(|c| {
        c.private_hosts.clear();
        c.extra_ports.clear();
    })
    .await;
    for host in [
        "127.0.0.1",
        "localhost",
        "169.254.169.254",
        "10.43.0.1",
        "[::1]",
    ] {
        let mut input = env.input();
        input.imap_host = host.to_string();
        let error = accounts::create(&env.app, OWNER, &input).await.unwrap_err();
        let Error::Upstream(text) = error else {
            panic!("{host}: {error:?}")
        };
        assert!(text.contains("cannot be used"), "{host}: {text}");
    }
    // Even a public name on a port that is not a mail port is not dialled.
    let mut input = env.input();
    input.imap_host = "8.8.8.8".into();
    input.imap_port = 5432;
    assert!(matches!(
        accounts::create(&env.app, OWNER, &input).await,
        Err(Error::Upstream(_))
    ));
    assert_eq!(env.imap.connections(), 0);
    assert!(accounts::list(&env.app, OWNER).await.unwrap().is_empty());
}

#[quench_test::test]
async fn the_connection_test_reports_each_half_and_is_rate_limited() {
    db!();
    let env = env().await;
    let ok = accounts::test(&env.app, OWNER, &env.input()).await.unwrap();
    assert!(ok.imap.ok && ok.smtp.ok, "{ok:?}");
    let mut input = env.input();
    input.smtp_port = env.imap.port();
    let half = accounts::test(&env.app, OWNER, &input).await.unwrap();
    assert!(half.imap.ok && !half.smtp.ok && !half.smtp.error.is_empty());
    let mut bad = env.input();
    bad.password = "wrong".into();
    let both = accounts::test(&env.app, OWNER, &bad).await.unwrap();
    assert!(!both.imap.ok && !both.smtp.ok);
    for _ in 0..9 {
        let _ = accounts::test(&env.app, OWNER, &env.input()).await;
    }
    assert!(matches!(
        accounts::test(&env.app, OWNER, &env.input()).await,
        Err(Error::Limited(_))
    ));
    // Another person has a budget of their own.
    assert!(accounts::test(&env.app, "bob", &env.input()).await.is_ok());
}

#[quench_test::test]
async fn nobody_else_can_read_change_or_remove_a_mailbox() {
    db!();
    let env = env().await;
    let account = accounts::create(&env.app, OWNER, &env.input())
        .await
        .unwrap();
    assert!(accounts::list(&env.app, "bob").await.unwrap().is_empty());
    assert!(matches!(
        accounts::update(&env.app, "bob", &account.id, &env.input()).await,
        Err(Error::NotFound)
    ));
    assert!(matches!(
        accounts::delete(&env.app, "bob", &account.id).await,
        Err(Error::NotFound)
    ));
    assert!(matches!(
        mailroom_service::mailbox::folders(&env.app, "bob", &account.id).await,
        Err(Error::NotFound)
    ));
    assert!(matches!(
        mailroom_service::mailbox::refresh(&env.app, "bob", &account.id).await,
        Err(Error::NotFound)
    ));
    assert_eq!(accounts::list(&env.app, OWNER).await.unwrap().len(), 1);
}

#[quench_test::test]
async fn changing_a_mailbox_keeps_the_password_when_left_blank() {
    db!();
    let env = env().await;
    let account = accounts::create(&env.app, OWNER, &env.input())
        .await
        .unwrap();
    let before = env
        .app
        .store
        .account_get(&account.id)
        .await
        .unwrap()
        .unwrap()
        .secret;
    let mut input = env.input();
    input.password = String::new();
    input.label = "Renamed".into();
    input.signature = "Ann".into();
    let updated = accounts::update(&env.app, OWNER, &account.id, &input)
        .await
        .unwrap();
    assert_eq!(updated.label, "Renamed");
    assert_eq!(updated.signature, "Ann");
    assert_eq!(
        env.app
            .store
            .account_get(&account.id)
            .await
            .unwrap()
            .unwrap()
            .secret,
        before
    );
    // A new password replaces it, sealed again.
    let mut input = env.input();
    input.password = PASSWORD.into();
    accounts::update(&env.app, OWNER, &account.id, &input)
        .await
        .unwrap();
    assert_ne!(
        env.app
            .store
            .account_get(&account.id)
            .await
            .unwrap()
            .unwrap()
            .secret,
        before
    );
    // A wrong one is refused and the stored one stays.
    let kept = env
        .app
        .store
        .account_get(&account.id)
        .await
        .unwrap()
        .unwrap()
        .secret;
    input.password = "wrong".into();
    assert!(matches!(
        accounts::update(&env.app, OWNER, &account.id, &input).await,
        Err(Error::Upstream(_))
    ));
    assert_eq!(
        env.app
            .store
            .account_get(&account.id)
            .await
            .unwrap()
            .unwrap()
            .secret,
        kept
    );
}

#[quench_test::test]
async fn the_number_of_mailboxes_per_person_is_capped() {
    db!();
    let env = env().await;
    let mut settings = env.app.settings();
    settings.max_accounts_per_user = 2;
    env.app.set_settings(settings);
    accounts::create(&env.app, OWNER, &env.input())
        .await
        .unwrap();
    accounts::create(&env.app, OWNER, &env.input())
        .await
        .unwrap();
    let Err(Error::Invalid(text)) = accounts::create(&env.app, OWNER, &env.input()).await else {
        panic!()
    };
    assert!(text.contains("at most 2"));
    assert!(
        accounts::create(&env.app, "bob", &env.input())
            .await
            .is_ok()
    );
}

#[quench_test::test]
async fn deleting_a_mailbox_removes_what_was_cached() {
    db!();
    let env = env().await;
    env.deliver("INBOX", "one", "body");
    let account = env.add_account(OWNER).await;
    assert_eq!(
        env.app
            .store
            .account_message_count(&account.id)
            .await
            .unwrap(),
        1
    );
    accounts::delete(&env.app, OWNER, &account.id)
        .await
        .unwrap();
    assert_eq!(env.app.store.messages_count().await.unwrap(), 0);
    assert!(
        env.app
            .store
            .folders_of(&account.id)
            .await
            .unwrap()
            .is_empty()
    );
}

#[quench_test::test]
async fn a_password_sealed_under_the_old_key_is_sealed_again_when_used() {
    db!();
    let old = env().await;
    let account = accounts::create(&old.app, OWNER, &old.input())
        .await
        .unwrap();
    let row = old
        .app
        .store
        .account_get(&account.id)
        .await
        .unwrap()
        .unwrap();
    // The same database, a rotated key.
    let mut config = (*old.app.config).clone();
    config.vault_key_previous = Some(config.vault_key.clone());
    config.vault_key = "rotated-key-0123456789abcdef0123456789".into();
    let rotated = mailroom_service::app::App {
        config: std::sync::Arc::new(config.clone()),
        vault: std::sync::Arc::new(mailroom_service::boot::vault(&config).unwrap()),
        ..old.app.clone()
    };
    let password = mailroom_service::connect::password(&rotated, &row)
        .await
        .unwrap();
    assert_eq!(password, PASSWORD);
    let after = old
        .app
        .store
        .account_get(&account.id)
        .await
        .unwrap()
        .unwrap();
    assert_ne!(after.secret, row.secret);
    // Now the current key alone opens it.
    let only_new = mailroom_service::vault::Vault::new(&config.vault_key, None).unwrap();
    assert!(
        only_new
            .open(
                &mailroom_service::vault::account_context(&row.id, &row.owner),
                &after.secret
            )
            .is_ok()
    );
}

#[quench_test::test]
fn the_provider_suggestions_cover_the_common_cases() {
    let all = accounts::presets();
    let gmail = all.iter().find(|p| p.id == "gmail").unwrap();
    assert_eq!(
        (
            gmail.imap_host.as_str(),
            gmail.imap_port,
            gmail.imap_security.as_str()
        ),
        ("imap.gmail.com", 993, "tls")
    );
    assert!(!gmail.save_sent, "Gmail keeps its own copy");
    assert!(all.iter().find(|p| p.id == "fastmail").unwrap().save_sent);
    assert!(
        all.iter()
            .any(|p| p.id == "proton-bridge" && p.note.contains("Bridge"))
    );
}
