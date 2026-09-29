use gatehouse_service::PublicBase;

#[test]
fn explicit_url_wins_and_loses_any_path_and_trailing_slash() {
    let base = PublicBase::resolve(
        "https://mail.example.com/",
        "https://other.example/gatehouse",
    );
    assert_eq!(base.as_str(), "https://mail.example.com");
}

#[test]
fn falls_back_to_the_origin_of_gatehouse_url() {
    let base = PublicBase::resolve("", "https://ennor.ddns.net/gatehouse");
    assert_eq!(base.as_str(), "https://ennor.ddns.net");
}

#[test]
fn keeps_a_non_default_port() {
    let base = PublicBase::resolve("", "http://localhost:5443/gatehouse");
    assert_eq!(base.as_str(), "http://localhost:5443");
}

#[test]
fn nothing_configured_uses_the_dev_default() {
    assert_eq!(
        PublicBase::resolve("", "").as_str(),
        "http://localhost:5443"
    );
}

#[test]
fn rejects_non_http_schemes_and_credentials_in_the_authority() {
    for bad in [
        "javascript://x",
        "ftp://host",
        "https://user@host",
        "not a url",
        "https://",
    ] {
        assert_eq!(
            PublicBase::resolve(bad, "").as_str(),
            "http://localhost:5443",
            "{bad}"
        );
    }
}
