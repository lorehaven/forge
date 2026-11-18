use mail_client::netguard::{Policy, Refusal, connect, is_public, resolve, valid_host};
use std::net::IpAddr;

fn ip(text: &str) -> IpAddr {
    text.parse().unwrap()
}

#[quench_test::test]
fn private_and_special_ipv4_addresses_are_not_public() {
    for bad in [
        "0.0.0.0",
        "10.0.0.1",
        "10.255.255.255",
        "127.0.0.1",
        "127.255.0.9",
        "100.64.0.1",
        "100.127.255.255",
        "169.254.169.254",
        "172.16.0.1",
        "172.31.255.255",
        "192.0.0.1",
        "192.0.2.5",
        "192.88.99.1",
        "192.168.1.1",
        "198.18.0.1",
        "198.19.255.255",
        "198.51.100.7",
        "203.0.113.9",
        "224.0.0.1",
        "239.255.255.255",
        "240.0.0.1",
        "255.255.255.255",
    ] {
        assert!(!is_public(ip(bad)), "{bad}");
    }
}

#[quench_test::test]
fn ordinary_public_ipv4_addresses_pass() {
    for good in [
        "8.8.8.8",
        "1.1.1.1",
        "74.125.0.1",
        "172.15.0.1",
        "172.32.0.1",
        "100.63.0.1",
        "100.128.0.1",
        "198.17.0.1",
        "198.20.0.1",
        "93.184.216.34",
    ] {
        assert!(is_public(ip(good)), "{good}");
    }
}

#[quench_test::test]
fn special_ipv6_addresses_are_not_public() {
    for bad in [
        "::",
        "::1",
        "fe80::1",
        "fc00::1",
        "fd12:3456::1",
        "ff02::1",
        "2001:db8::1",
        "2001::1",
        "feC0::1",
        "100::1",
        "::ffff:127.0.0.1",
        "::ffff:10.0.0.1",
        "::ffff:192.168.0.1",
        "::ffff:169.254.169.254",
        "64:ff9b::7f00:1",
        "64:ff9b::a00:1",
        "2002:7f00:1::1",
        "2002:a9fe:a9fe::1",
        "4000::1",
        "3fff::1",
    ] {
        assert!(!is_public(ip(bad)), "{bad}");
    }
}

#[quench_test::test]
fn public_ipv6_addresses_pass() {
    for good in [
        "2606:4700:4700::1111",
        "2a00:1450:4001::200e",
        "::ffff:8.8.8.8",
        "64:ff9b::808:808",
        "2002:808:808::1",
    ] {
        assert!(is_public(ip(good)), "{good}");
    }
}

#[quench_test::test]
fn host_names_are_validated() {
    for ok in [
        "imap.gmail.com",
        "a-b.c",
        "10.1.2.3",
        "[::1]",
        "::1",
        "host_name.local",
    ] {
        assert!(valid_host(ok), "{ok}");
    }
    for bad in [
        "",
        ".x",
        "-x",
        "a b",
        "a/b",
        "a@b",
        "http://x",
        "a:80/x?y",
        "x\r\ny",
        &"a".repeat(300),
    ] {
        // Colons are allowed (IPv6), so "a:80/x?y" fails on the slash.
        assert!(!valid_host(bad), "{bad:?}");
    }
}

#[quench_test::test]
fn the_policy_matches_exact_names_and_suffixes() {
    let policy = Policy {
        private_hosts: vec![
            "bridge.mail.svc".into(),
            "*.internal.example".into(),
            " Upper.Case ".into(),
        ],
        ..Policy::default()
    };
    assert!(policy.allows_private("bridge.mail.svc"));
    assert!(policy.allows_private("BRIDGE.mail.svc."));
    assert!(policy.allows_private("a.internal.example"));
    assert!(policy.allows_private("internal.example"));
    assert!(policy.allows_private("upper.case"));
    assert!(!policy.allows_private("evilinternal.example"));
    assert!(!policy.allows_private("other.mail.svc"));
    assert!(!Policy::default().allows_private("anything"));
}

#[quench_test::test]
fn only_mail_ports_unless_extended() {
    let policy = Policy {
        extra_ports: vec![1143],
        ..Policy::default()
    };
    for port in [143, 993, 465, 587, 2525, 1143] {
        assert!(policy.allows_port(port), "{port}");
    }
    for port in [22, 25, 80, 443, 5432, 6379, 6443, 8080, 10250] {
        assert!(!policy.allows_port(port), "{port}");
    }
}

#[quench_test::test]
async fn resolve_refuses_internal_targets_before_connecting() {
    let policy = Policy::default();
    assert_eq!(
        resolve("127.0.0.1", 993, &policy).await,
        Err(Refusal::NotPublic)
    );
    assert_eq!(
        resolve("localhost", 993, &policy).await,
        Err(Refusal::NotPublic)
    );
    assert_eq!(
        resolve("169.254.169.254", 143, &policy).await,
        Err(Refusal::NotPublic)
    );
    assert_eq!(
        resolve("[::1]", 993, &policy).await,
        Err(Refusal::NotPublic)
    );
    assert_eq!(
        resolve("10.43.0.1", 993, &policy).await,
        Err(Refusal::NotPublic)
    );
    assert_eq!(
        resolve("8.8.8.8", 5432, &policy).await,
        Err(Refusal::Port(5432))
    );
    assert_eq!(
        resolve("8.8.8.8", 6443, &policy).await,
        Err(Refusal::Port(6443))
    );
    assert_eq!(
        resolve("bad host", 993, &policy).await,
        Err(Refusal::BadHost)
    );
    assert_eq!(resolve("", 993, &policy).await, Err(Refusal::BadHost));
    assert!(resolve("8.8.8.8", 993, &policy).await.is_ok());
    assert!(resolve("2606:4700:4700::1111", 993, &policy).await.is_ok());
}

#[quench_test::test]
async fn the_allow_list_admits_a_private_host() {
    let policy = Policy {
        private_hosts: vec!["127.0.0.1".into()],
        extra_ports: vec![1143],
        ..Policy::default()
    };
    let addresses = resolve("127.0.0.1", 1143, &policy).await.unwrap();
    assert_eq!(addresses[0].port(), 1143);
    // The port rule still applies to an allowed host.
    assert_eq!(
        resolve("127.0.0.1", 5432, &policy).await,
        Err(Refusal::Port(5432))
    );
}

#[quench_test::test]
async fn an_unresolvable_name_is_a_refusal_not_a_panic() {
    let policy = Policy::default();
    let result = resolve("no-such-host.invalid", 993, &policy).await;
    assert_eq!(result, Err(Refusal::NoAddress));
}

#[quench_test::test]
async fn connect_goes_only_where_it_may() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let denied = connect("127.0.0.1", port, &Policy::default())
        .await
        .unwrap_err();
    assert_eq!(denied.kind(), std::io::ErrorKind::PermissionDenied);
    let allowed = Policy {
        private_hosts: vec!["127.0.0.1".into()],
        extra_ports: vec![port],
        ..Policy::default()
    };
    assert!(connect("127.0.0.1", port, &allowed).await.is_ok());
    // Nothing listens on the next port: the error is a connection error, not a refusal.
    drop(listener);
    let closed = connect("127.0.0.1", port, &allowed).await.unwrap_err();
    assert_ne!(closed.kind(), std::io::ErrorKind::PermissionDenied);
}

#[quench_test::test]
fn refusals_have_user_safe_text() {
    for refusal in [
        Refusal::BadHost,
        Refusal::Port(1),
        Refusal::NoAddress,
        Refusal::NotPublic,
    ] {
        let text = refusal.to_string();
        assert!(
            !text.is_empty() && !text.contains("127.") && !text.contains("10."),
            "{text}"
        );
    }
}
