#![allow(clippy::unwrap_used)]

use gauge_app::{Msg, Request};
use gauge_core::{HostCard, HostDetail, HostSummary};
use gauge_web::protocol::{base_of, interpret, segment, unreachable, url_for};
use quench_serde::json;

fn summary() -> HostSummary {
    HostSummary {
        name: "lorien".into(),
        kind: "pcp".into(),
        url: "http://lorien".into(),
        up: true,
        error: None,
        last_ok: Some(1.0),
        view: None,
    }
}

#[quench_test::test]
fn the_service_prefix_is_what_comes_before_ui() {
    assert_eq!(base_of("/gauge/ui/home"), "/gauge");
    assert_eq!(base_of("/gauge/ui/hosts/lorien"), "/gauge");
    assert_eq!(base_of("/gauge/ui"), "/gauge");
    assert_eq!(base_of("/ui/home"), "");
    assert_eq!(base_of("/a/b/ui/home"), "/a/b");
    assert_eq!(base_of("/elsewhere"), "");
}

#[quench_test::test]
fn the_requests_go_to_the_api_under_the_prefix() {
    assert_eq!(
        url_for("/gauge", &Request::Hosts).as_deref(),
        Some("/gauge/api/v1/hosts")
    );
    assert_eq!(
        url_for("", &Request::Host("lorien".into())).as_deref(),
        Some("/api/v1/hosts/lorien")
    );
    assert_eq!(
        url_for("/gauge", &Request::Host("a b/c".into())).as_deref(),
        Some("/gauge/api/v1/hosts/a%20b%2Fc")
    );
    assert_eq!(url_for("/gauge", &Request::Language("de-DE".into())), None);
    assert_eq!(segment("a.b-c_d~e"), "a.b-c_d~e");
}

#[quench_test::test]
fn the_hosts_answer_becomes_cards() {
    let body = json::to_string(&vec![HostCard {
        host: summary(),
        cpu: vec![Some(1.0), None],
    }]);
    let Msg::Hosts(cards) = interpret(&Request::Hosts, 200, &body) else {
        panic!("not hosts")
    };
    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0].cpu, vec![Some(1.0), None]);
}

#[quench_test::test]
fn a_hosts_answer_becomes_its_detail() {
    let body = json::to_string(&HostDetail {
        host: summary(),
        history: vec![],
    });
    let Msg::Detail(d) = interpret(&Request::Host("lorien".into()), 200, &body) else {
        panic!("not a detail")
    };
    assert_eq!(d.host.name, "lorien");
}

#[quench_test::test]
fn the_statuses_mean_what_a_screen_can_say() {
    let hosts = Request::Hosts;
    let one = Request::Host("lorien".into());
    assert_eq!(interpret(&hosts, 401, ""), Msg::SignedOut);
    assert_eq!(interpret(&one, 404, ""), Msg::NotFound("lorien".into()));
    assert!(matches!(interpret(&hosts, 404, ""), Msg::Failed(_)));
    assert!(matches!(interpret(&hosts, 403, ""), Msg::Failed(m) if m.contains("gatehouse")));
    assert_eq!(
        interpret(&hosts, 502, ""),
        Msg::Failed("the service answered 502".into())
    );
    assert!(
        matches!(interpret(&hosts, 200, "<html>login</html>"), Msg::Failed(m) if m.contains("unreadable"))
    );
    assert_eq!(
        unreachable("offline"),
        Msg::Failed("cannot reach the service: offline".into())
    );
}

#[quench_test::test]
fn the_stylesheet_is_the_apps() {
    assert!(gauge_web::stylesheet().contains(".gg-meter"));
}

quench_test::main!();
