//! The address a share link is handed out with.

use warehouse_service::routers::ui::api::shares::PublicBase;

#[quench_test::test]
fn a_configured_address_wins_and_loses_its_trailing_slash() {
    let base = PublicBase::from_parts(
        " https://files.example.test/warehouse/ ",
        Some("http"),
        Some("other.example"),
        "/warehouse",
    );
    assert_eq!(base.0, "https://files.example.test/warehouse");
    assert_eq!(
        base.link("T0KEN"),
        "https://files.example.test/warehouse/share/T0KEN"
    );
}

#[quench_test::test]
fn without_one_the_request_says_where_it_came_in() {
    let base = PublicBase::from_parts("", Some("https"), Some("ennor.ddns.net"), "/warehouse");
    assert_eq!(base.link("T"), "https://ennor.ddns.net/warehouse/share/T");

    // The path prefix may be written with or without its slashes; none at all is the root.
    assert_eq!(
        PublicBase::from_parts("", Some("https"), Some("h"), "warehouse/").0,
        "https://h/warehouse"
    );
    assert_eq!(
        PublicBase::from_parts("", Some("https"), Some("h"), "/").0,
        "https://h"
    );
    assert_eq!(
        PublicBase::from_parts("", Some("https"), Some("h:8443"), "").link("T"),
        "https://h:8443/share/T"
    );
}

#[quench_test::test]
fn a_chain_of_proxies_is_read_from_the_front_and_only_http_or_https_is_believed() {
    let base = PublicBase::from_parts(
        "",
        Some("http, https"),
        Some("public.example, internal.svc"),
        "/w",
    );
    assert_eq!(base.0, "http://public.example/w");
    // Anything but `http` is `https`: a header cannot make the link say `javascript:`.
    assert_eq!(
        PublicBase::from_parts("", Some("javascript"), Some("h"), "").0,
        "https://h"
    );
    assert_eq!(
        PublicBase::from_parts("", None, Some("h"), "").0,
        "https://h"
    );
}

#[quench_test::test]
fn with_no_host_at_all_the_link_is_the_part_after_it() {
    let base = PublicBase::from_parts("", None, None, "/warehouse");
    assert_eq!(base.link("T"), "/warehouse/share/T");
    assert_eq!(
        PublicBase::from_parts("", None, Some("  "), "").link("T"),
        "/share/T"
    );
}
