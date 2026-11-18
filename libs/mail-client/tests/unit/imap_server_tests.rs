use mail_client::imap::{Auth, Client, Command, Config, Error, Search, Status};
use mail_client::netguard::Policy;
use mail_client::presets::Security;
use mail_testkit::{CERT_PEM, ImapOptions, ImapServer};

fn message(subject: &str, from: &str, body: &str) -> Vec<u8> {
    format!("From: {from}\r\nTo: me@example.com\r\nSubject: {subject}\r\nDate: Tue, 29 Sep 2026 12:00:00 +0000\r\nMessage-ID: <{subject}@x>\r\n\r\n{body}\r\n").into_bytes()
}

async fn server(options: ImapOptions) -> ImapServer {
    let server = ImapServer::start(options).await;
    server.add_account("me@example.com", "s3cret");
    server.add_folder("me@example.com", "Sent", &["\\Sent"]);
    server.add_folder("me@example.com", "Wysłane/Zażółć", &[]);
    server
}

fn config(server: &ImapServer, security: Security) -> Config {
    let mut config = Config::new(
        "127.0.0.1",
        server.port(),
        security,
        "me@example.com",
        Auth::Password("s3cret".into()),
    );
    config.policy = Policy {
        private_hosts: vec!["127.0.0.1".into()],
        extra_ports: vec![server.port()],
        ..Policy::default()
    };
    config.tls_roots_pem = vec![CERT_PEM.to_string()];
    config.tls_server_name = Some("localhost".into());
    config.timeout = std::time::Duration::from_secs(5);
    config
}

async fn connect(server: &ImapServer) -> Client {
    Client::connect(&config(server, Security::Tls))
        .await
        .unwrap()
}

#[quench_test::test]
async fn logs_in_over_implicit_tls_with_plain_auth() {
    let s = server(ImapOptions::default()).await;
    let client = connect(&s).await;
    assert!(client.capabilities().iter().any(|c| c == "IMAP4rev1"));
    assert_eq!(s.logins(), 1);
    assert!(s.log().iter().any(|l| l.contains("AUTHENTICATE PLAIN")));
    client.logout().await;
}

#[quench_test::test]
async fn falls_back_to_login_when_plain_is_not_offered() {
    let s = server(ImapOptions {
        auth_plain: false,
        ..ImapOptions::default()
    })
    .await;
    let client = connect(&s).await;
    assert!(
        s.log()
            .iter()
            .any(|l| l.contains("LOGIN \"me@example.com\" \"s3cret\""))
    );
    client.logout().await;
}

#[quench_test::test]
async fn upgrades_with_starttls_before_sending_the_password() {
    let s = server(ImapOptions {
        implicit_tls: false,
        starttls: true,
        logindisabled_before_tls: true,
        ..ImapOptions::default()
    })
    .await;
    let client = Client::connect(&config(&s, Security::StartTls))
        .await
        .unwrap();
    let log = s.log();
    let starttls = log.iter().position(|l| l.contains("STARTTLS")).unwrap();
    let auth = log
        .iter()
        .position(|l| l.contains("AUTHENTICATE") || l.contains("LOGIN"))
        .unwrap();
    assert!(starttls < auth, "{log:?}");
    client.logout().await;
}

#[quench_test::test]
async fn refuses_to_send_a_password_without_a_secure_connection() {
    // The server offers no STARTTLS and is plain: the client must not log in.
    let s = server(ImapOptions {
        implicit_tls: false,
        starttls: false,
        ..ImapOptions::default()
    })
    .await;
    let result = Client::connect(&config(&s, Security::StartTls)).await;
    assert!(matches!(result, Err(Error::Insecure)), "{result:?}");
    assert_eq!(s.logins(), 0);
    assert!(!s.log().iter().any(|l| l.contains("s3cret")));
}

#[quench_test::test]
async fn a_wrong_password_is_an_auth_error_and_names_no_secret() {
    let s = server(ImapOptions::default()).await;
    let mut cfg = config(&s, Security::Tls);
    cfg.auth = Auth::Password("wrong".into());
    let error = Client::connect(&cfg).await.unwrap_err();
    assert!(matches!(error, Error::Auth(_)), "{error:?}");
    assert!(!error.to_string().contains("wrong"));
    assert_eq!(s.failed_logins(), 1);
    assert!(!format!("{cfg:?}").contains("wrong"));
}

#[quench_test::test]
async fn xoauth2_logs_in_with_a_token() {
    let s = server(ImapOptions {
        xoauth2: true,
        ..ImapOptions::default()
    })
    .await;
    s.set_token("me@example.com", "tok123");
    let mut cfg = config(&s, Security::Tls);
    cfg.auth = Auth::XOAuth2("tok123".into());
    Client::connect(&cfg).await.unwrap().logout().await;
    cfg.auth = Auth::XOAuth2("stale".into());
    assert!(matches!(Client::connect(&cfg).await, Err(Error::Auth(_))));
    let plain = server(ImapOptions::default()).await;
    let mut cfg = config(&plain, Security::Tls);
    cfg.auth = Auth::XOAuth2("x".into());
    assert!(matches!(Client::connect(&cfg).await, Err(Error::Auth(_))));
}

#[quench_test::test]
async fn an_untrusted_certificate_is_refused() {
    let s = server(ImapOptions::default()).await;
    let mut cfg = config(&s, Security::Tls);
    cfg.tls_roots_pem.clear();
    let error = Client::connect(&cfg).await.unwrap_err();
    assert!(matches!(error, Error::Tls(_)), "{error:?}");
    assert_eq!(s.logins(), 0);
    let mut cfg = config(&s, Security::Tls);
    cfg.tls_server_name = Some("other.example".into());
    assert!(matches!(Client::connect(&cfg).await, Err(Error::Tls(_))));
}

#[quench_test::test]
async fn the_address_policy_applies_before_any_connection() {
    let s = server(ImapOptions::default()).await;
    let mut cfg = config(&s, Security::Tls);
    cfg.policy = Policy::default();
    let error = Client::connect(&cfg).await.unwrap_err();
    assert!(matches!(error, Error::Refused(_)), "{error:?}");
    assert_eq!(s.connections(), 0);
}

#[quench_test::test]
async fn lists_folders_with_roles_and_decoded_names() {
    let s = server(ImapOptions::default()).await;
    let mut client = connect(&s).await;
    let folders = client.list_folders().await.unwrap();
    let names: Vec<_> = folders.iter().map(|f| f.path.as_str()).collect();
    assert!(
        names.contains(&"INBOX") && names.contains(&"Sent") && names.contains(&"Wysłane/Zażółć"),
        "{names:?}"
    );
    assert_eq!(
        folders.iter().find(|f| f.path == "Sent").unwrap().role(),
        "sent"
    );
    client.logout().await;
}

#[quench_test::test]
async fn selects_and_fetches_headers_and_flags() {
    let s = server(ImapOptions::default()).await;
    let uid1 = s.deliver(
        "me@example.com",
        "INBOX",
        &message("one", "Alice <a@x.org>", "first"),
        &["\\Seen"],
    );
    let uid2 = s.deliver(
        "me@example.com",
        "INBOX",
        &message("two", "Bob <b@x.org>", "second"),
        &[],
    );
    let mut client = connect(&s).await;
    let selected = client.select("INBOX", true).await.unwrap();
    assert_eq!(selected.exists, 2);
    assert_eq!(selected.uid_next, uid2 + 1);
    assert_eq!(selected.uid_validity, 1000);
    assert!(selected.read_only);
    let fetched = client
        .fetch_headers("1:*", "FROM TO SUBJECT DATE MESSAGE-ID")
        .await
        .unwrap();
    assert_eq!(fetched.len(), 2);
    assert_eq!(fetched[0].uid, uid1);
    assert_eq!(fetched[0].flags, ["\\Seen"]);
    assert!(fetched[1].flags.is_empty());
    assert!(fetched[0].size > 50);
    assert!(fetched[0].internal_date.is_some());
    let summary = mail_client::message::summarize(&fetched[1].data);
    assert_eq!(summary.subject, "two");
    assert_eq!(summary.from[0].name, "Bob");
    let flags = client.fetch_flags("1:*").await.unwrap();
    assert_eq!(
        flags,
        vec![(uid1, vec!["\\Seen".to_string()]), (uid2, vec![])]
    );
    client.logout().await;
}

#[quench_test::test]
async fn fetches_a_whole_message_without_marking_it_read() {
    let s = server(ImapOptions::default()).await;
    let raw = message("big", "a@x.org", &"line\r\n".repeat(20_000));
    let uid = s.deliver("me@example.com", "INBOX", &raw, &[]);
    let mut client = connect(&s).await;
    client.select("INBOX", false).await.unwrap();
    let body = client.fetch_message(uid).await.unwrap().unwrap();
    assert_eq!(body, raw);
    assert!(s.messages("me@example.com", "INBOX")[0].flags.is_empty());
    assert!(client.fetch_message(999).await.unwrap().is_none());
    client.logout().await;
}

#[quench_test::test]
async fn searches_by_uid_state_and_text() {
    let s = server(ImapOptions::default()).await;
    let a = s.deliver(
        "me@example.com",
        "INBOX",
        &message("invoice", "billing@x.org", "pay now"),
        &["\\Seen"],
    );
    let b = s.deliver(
        "me@example.com",
        "INBOX",
        &message("lunch", "friend@x.org", "noon?"),
        &[],
    );
    let c = s.deliver(
        "me@example.com",
        "INBOX",
        &message("zażółć", "z@x.org", "gęślą"),
        &[],
    );
    let mut client = connect(&s).await;
    client.select("INBOX", true).await.unwrap();
    assert_eq!(client.uid_search(Search::all()).await.unwrap(), [a, b, c]);
    assert_eq!(client.uid_search(Search::unseen()).await.unwrap(), [b, c]);
    assert_eq!(
        client.uid_search(Search::uid_from(b)).await.unwrap(),
        [b, c]
    );
    assert_eq!(
        client.uid_search(Search::text("invoice")).await.unwrap(),
        [a]
    );
    assert_eq!(client.uid_search(Search::text("NOON")).await.unwrap(), [b]);
    assert_eq!(client.uid_search(Search::text("gęślą")).await.unwrap(), [c]);
    assert!(
        client
            .uid_search(Search::text("nothing like this"))
            .await
            .unwrap()
            .is_empty()
    );
    client.logout().await;
}

#[quench_test::test]
async fn stores_flags_and_refuses_on_a_read_only_folder() {
    let s = server(ImapOptions::default()).await;
    let uid = s.deliver(
        "me@example.com",
        "INBOX",
        &message("m", "a@x.org", "b"),
        &[],
    );
    let mut client = connect(&s).await;
    client.select("INBOX", true).await.unwrap();
    assert!(matches!(
        client.store_flags(&uid.to_string(), "+", &["\\Seen"]).await,
        Err(Error::No(_))
    ));
    client.select("INBOX", false).await.unwrap();
    client
        .store_flags(&uid.to_string(), "+", &["\\Seen", "\\Flagged"])
        .await
        .unwrap();
    assert_eq!(
        s.messages("me@example.com", "INBOX")[0].flags,
        ["\\Seen", "\\Flagged"]
    );
    client
        .store_flags(&uid.to_string(), "-", &["\\Seen"])
        .await
        .unwrap();
    assert_eq!(
        s.messages("me@example.com", "INBOX")[0].flags,
        ["\\Flagged"]
    );
    client
        .store_flags(&uid.to_string(), "", &["$Forwarded"])
        .await
        .unwrap();
    assert_eq!(
        s.messages("me@example.com", "INBOX")[0].flags,
        ["$Forwarded"]
    );
    client.logout().await;
}

#[quench_test::test]
async fn moves_copies_and_expunges() {
    for move_ext in [true, false] {
        let s = server(ImapOptions {
            move_ext,
            ..ImapOptions::default()
        })
        .await;
        let a = s.deliver(
            "me@example.com",
            "INBOX",
            &message("a", "a@x.org", "1"),
            &[],
        );
        let b = s.deliver(
            "me@example.com",
            "INBOX",
            &message("b", "a@x.org", "2"),
            &[],
        );
        let mut client = connect(&s).await;
        client.select("INBOX", false).await.unwrap();
        client
            .move_to(&a.to_string(), "Wysłane/Zażółć")
            .await
            .unwrap();
        assert_eq!(
            s.messages("me@example.com", "INBOX").len(),
            1,
            "move_ext={move_ext}"
        );
        assert_eq!(s.messages("me@example.com", "Wysłane/Zażółć").len(), 1);
        client.copy(&b.to_string(), "Sent").await.unwrap();
        assert_eq!(s.messages("me@example.com", "INBOX").len(), 1);
        assert_eq!(s.messages("me@example.com", "Sent").len(), 1);
        client
            .store_flags(&b.to_string(), "+", &["\\Deleted"])
            .await
            .unwrap();
        client.expunge(None).await.unwrap();
        assert!(s.messages("me@example.com", "INBOX").is_empty());
        assert!(matches!(
            client.copy(&b.to_string(), "Missing").await,
            Err(Error::No(_))
        ));
        client.logout().await;
    }
}

#[quench_test::test]
async fn uid_expunge_removes_only_the_named_message() {
    let s = server(ImapOptions::default()).await;
    let a = s.deliver(
        "me@example.com",
        "INBOX",
        &message("a", "a@x.org", "1"),
        &["\\Deleted"],
    );
    let b = s.deliver(
        "me@example.com",
        "INBOX",
        &message("b", "a@x.org", "2"),
        &["\\Deleted"],
    );
    let mut client = connect(&s).await;
    client.select("INBOX", false).await.unwrap();
    client.expunge(Some(&a.to_string())).await.unwrap();
    let left = s.messages("me@example.com", "INBOX");
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].uid, b);
    client.logout().await;
}

#[quench_test::test]
async fn appends_a_message_and_reports_its_uid() {
    let s = server(ImapOptions::default()).await;
    let mut client = connect(&s).await;
    let raw = message("sent copy", "me@example.com", "body\r\nwith lines");
    let uid = client
        .append("Sent", &["\\Seen"], raw.clone())
        .await
        .unwrap();
    assert_eq!(uid, Some(1));
    let stored = s.messages("me@example.com", "Sent");
    assert_eq!(stored[0].raw, raw);
    assert_eq!(stored[0].flags, ["\\Seen"]);
    assert!(matches!(
        client.append("Missing", &[], raw).await,
        Err(Error::No(_))
    ));
    // Folder names with non-ASCII characters go as modified UTF-7.
    client
        .append("Wysłane/Zażółć", &[], message("x", "a@x.org", "y"))
        .await
        .unwrap();
    assert_eq!(s.messages("me@example.com", "Wysłane/Zażółć").len(), 1);
    client.create_folder("Nowy/Folder").await.unwrap();
    let names: Vec<_> = client
        .list_folders()
        .await
        .unwrap()
        .into_iter()
        .map(|f| f.path)
        .collect();
    assert!(names.contains(&"Nowy/Folder".to_string()));
    client.logout().await;
}

#[quench_test::test]
async fn selecting_a_missing_folder_is_a_no() {
    let s = server(ImapOptions::default()).await;
    let mut client = connect(&s).await;
    assert!(matches!(
        client.select("Nope", true).await,
        Err(Error::No(_))
    ));
    client.noop().await.unwrap();
    client.logout().await;
}

#[quench_test::test]
async fn arguments_that_could_inject_commands_are_refused_or_quoted() {
    let s = server(ImapOptions::default()).await;
    s.deliver(
        "me@example.com",
        "INBOX",
        &message("m", "a@x.org", "b"),
        &[],
    );
    let mut client = connect(&s).await;
    client.select("INBOX", false).await.unwrap();
    for set in ["1 2", "1;", "1\r\nx", "", "a", "1:*)"] {
        assert!(
            matches!(client.fetch_flags(set).await, Err(Error::Protocol(_))),
            "{set:?}"
        );
        assert!(client.copy(set, "Sent").await.is_err());
    }
    for flag in ["\\Seen)", "x y", "", "\\"] {
        assert!(
            matches!(
                client.store_flags("1", "+", &[flag]).await,
                Err(Error::Protocol(_))
            ),
            "{flag:?}"
        );
    }
    assert!(matches!(
        client.store_flags("1", "*", &["\\Seen"]).await,
        Err(Error::Protocol(_))
    ));
    // A hostile folder name or search text is quoted or sent as a literal and never ends the command early.
    let before = s.log().len();
    let _ = client.select("INBOX\" UID FETCH 1:* (BODY[])", true).await;
    let _ = client.uid_search(Search::text("x\r\nA1 LOGOUT")).await;
    let _ = client
        .uid_search(Search::text("quote\" and \\ backslash"))
        .await;
    let log = s.log();
    assert!(
        !log[before..].iter().any(|l| l.trim() == "A1 LOGOUT"),
        "{:?}",
        &log[before..]
    );
    client.noop().await.unwrap();
    client.logout().await;
}

#[quench_test::test]
async fn a_hostile_server_answering_with_the_wrong_uid_gets_nothing_accepted() {
    let s = server(ImapOptions {
        hostile_fetch: true,
        ..ImapOptions::default()
    })
    .await;
    let uid = s.deliver(
        "me@example.com",
        "INBOX",
        &message("m", "a@x.org", "b"),
        &[],
    );
    let mut client = connect(&s).await;
    client.select("INBOX", true).await.unwrap();
    assert!(client.fetch_message(uid).await.unwrap().is_none());
    client.logout().await;
}

#[quench_test::test]
async fn a_server_that_hangs_up_is_an_io_error() {
    let s = server(ImapOptions {
        drop_after_commands: 3,
        ..ImapOptions::default()
    })
    .await;
    let mut client = connect(&s).await;
    let mut failed = false;
    for _ in 0..5 {
        if client.noop().await.is_err() {
            failed = true;
            break;
        }
    }
    assert!(failed);
}

#[quench_test::test]
async fn silence_is_a_timeout_not_a_hang() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        let (_socket, _) = listener.accept().await.unwrap();
        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
    });
    let mut cfg = Config::new(
        "127.0.0.1",
        port,
        Security::StartTls,
        "u",
        Auth::Password("p".into()),
    );
    cfg.policy = Policy {
        private_hosts: vec!["127.0.0.1".into()],
        extra_ports: vec![port],
        ..Policy::default()
    };
    cfg.timeout = std::time::Duration::from_millis(300);
    let started = std::time::Instant::now();
    let error = Client::connect(&cfg).await.unwrap_err();
    assert!(matches!(error, Error::Timeout), "{error:?}");
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
}

/// A server that sends hostile responses, to show the reader's limits hold.
async fn hostile(script: Vec<u8>) -> Result<Client, Error> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let (mut socket, _) = listener.accept().await.unwrap();
        let _ = socket.write_all(&script).await;
        let mut sink = [0u8; 1024];
        while socket.read(&mut sink).await.is_ok_and(|n| n > 0) {}
    });
    let mut cfg = Config::new(
        "127.0.0.1",
        port,
        Security::StartTls,
        "u",
        Auth::Password("p".into()),
    );
    cfg.policy = Policy {
        private_hosts: vec!["127.0.0.1".into()],
        extra_ports: vec![port],
        ..Policy::default()
    };
    cfg.timeout = std::time::Duration::from_secs(2);
    cfg.max_response = 1024 * 1024;
    Client::connect(&cfg).await
}

#[quench_test::test]
async fn endless_lines_and_huge_literals_and_deep_lists_are_refused() {
    let mut endless = b"* OK ".to_vec();
    endless.extend(std::iter::repeat_n(b'a', 600 * 1024));
    assert!(matches!(hostile(endless).await, Err(Error::Protocol(_))));

    let huge = b"* OK [CAPABILITY IMAP4rev1] x {999999999}\r\n".to_vec();
    assert!(matches!(hostile(huge).await, Err(Error::Protocol(_))));

    let bye = b"* BYE go away\r\n".to_vec();
    assert!(matches!(hostile(bye).await, Err(Error::Bye(_))));

    let not_imap = b"HTTP/1.1 400 Bad Request\r\n".to_vec();
    let result = hostile(not_imap).await;
    assert!(result.is_err());

    // A greeting without capabilities, then deeply nested garbage as the CAPABILITY answer.
    let mut deep = b"* OK hi\r\n".to_vec();
    deep.extend(b"* ");
    deep.extend(std::iter::repeat_n(b'(', 1000));
    deep.extend(b"\r\n");
    assert!(hostile(deep).await.is_err());
}

#[quench_test::test]
async fn command_builder_never_lets_input_end_the_command() {
    // `Command` only ever produces quoted strings or literals for user text; this guards the quoting itself.
    let s = server(ImapOptions::default()).await;
    let mut client = connect(&s).await;
    let done = client
        .run(Command::new("SELECT ").quoted("IN\"BOX"))
        .await
        .unwrap();
    assert_eq!(done.status, Status::No);
    assert!(done.clone().require_ok().is_err());
    let done = client.run(Command::new("NOOP")).await.unwrap();
    assert_eq!(done.status, Status::Ok);
    let bad = client.run(Command::new("FROBNICATE")).await.unwrap();
    assert_eq!(bad.status, Status::Bad);
    assert!(matches!(bad.require_ok(), Err(Error::Bad(_))));
    client.logout().await;
}

#[quench_test::test]
fn errors_have_readable_text() {
    for error in [
        Error::Io(std::io::ErrorKind::BrokenPipe.into()),
        Error::Tls("x".into()),
        Error::Timeout,
        Error::Protocol("p".into()),
        Error::Refused(mail_client::netguard::Refusal::NotPublic),
        Error::No("n".into()),
        Error::Bad("b".into()),
        Error::Auth("a".into()),
        Error::Bye("bye".into()),
        Error::Insecure,
    ] {
        assert!(!error.to_string().is_empty());
    }
    assert!(format!("{:?}", Auth::Password("topsecret".into())).contains("redacted"));
    assert!(!format!("{:?}", Auth::XOAuth2("topsecret".into())).contains("topsecret"));
}
