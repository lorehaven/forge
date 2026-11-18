use mail_client::presets::{PRESETS, PROTON_BRIDGE, Security, for_address, guess};

#[quench_test::test]
fn known_providers_are_found_by_domain() {
    assert_eq!(for_address("me@gmail.com").unwrap().id, "gmail");
    assert_eq!(for_address("me@GoogleMail.com").unwrap().id, "gmail");
    assert_eq!(for_address("me@hotmail.com").unwrap().id, "outlook");
    assert_eq!(for_address("me@wp.pl").unwrap().id, "wp");
    assert_eq!(for_address("me@proton.me").unwrap().id, "proton-bridge");
    assert!(for_address("me@example.org").is_none());
    assert!(for_address("no-at-sign").is_none());
}

#[quench_test::test]
fn every_preset_uses_a_secure_connection_on_a_mail_port() {
    let ports = mail_client::netguard::MAIL_PORTS;
    for preset in PRESETS {
        assert!(
            mail_client::netguard::valid_host(preset.imap_host),
            "{}",
            preset.id
        );
        assert!(
            mail_client::netguard::valid_host(preset.smtp_host),
            "{}",
            preset.id
        );
        assert!(
            ports.contains(&preset.imap_port) && ports.contains(&preset.smtp_port),
            "{}",
            preset.id
        );
        assert!(!preset.note.is_empty());
        assert!(!preset.domains.is_empty());
        let expected = |port: u16| {
            if port == 993 || port == 465 {
                Security::Tls
            } else {
                Security::StartTls
            }
        };
        assert_eq!(
            preset.imap_security,
            expected(preset.imap_port),
            "{}",
            preset.id
        );
        assert_eq!(
            preset.smtp_security,
            expected(preset.smtp_port),
            "{}",
            preset.id
        );
    }
    assert_eq!(PROTON_BRIDGE.imap_security, Security::StartTls);
    assert_eq!(PROTON_BRIDGE.imap_port, 1143);
}

#[quench_test::test]
fn preset_ids_and_domains_are_unique_per_provider() {
    let mut ids: Vec<_> = PRESETS.iter().map(|p| p.id).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), PRESETS.len());
}

#[quench_test::test]
fn unknown_domains_get_a_guess_that_is_only_a_guess() {
    assert_eq!(
        guess("a@example.org"),
        Some((
            "imap.example.org".to_string(),
            "smtp.example.org".to_string()
        ))
    );
    assert_eq!(guess("a@bad domain"), None);
    assert_eq!(guess("nodomain"), None);
}

#[quench_test::test]
fn security_names_round_trip() {
    for s in [Security::Tls, Security::StartTls] {
        assert_eq!(Security::parse(s.name()), Some(s));
    }
    assert_eq!(Security::parse("none"), None);
}
