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

#[quench_test::test]
fn the_stored_history_is_asked_by_range_and_read_back() {
    use gauge_core::{HostHistory, Point, Range};
    let ask = Request::History("a b".into(), Range::Week);
    assert_eq!(
        url_for("/gauge", &ask).as_deref(),
        Some("/gauge/api/v1/hosts/a%20b/history?range=7d")
    );
    let body = json::to_string(&HostHistory {
        name: "a b".into(),
        range: "7d".into(),
        step_secs: 1800,
        points: vec![Point::gap(1.0)],
    });
    let Msg::History(h) = interpret(&ask, 200, &body) else {
        panic!("not history")
    };
    assert_eq!((h.range.as_str(), h.points.len()), ("7d", 1));
}

#[quench_test::test]
fn no_database_is_not_a_failure_of_the_screen() {
    use gauge_core::Range;
    let ask = Request::History("lorien".into(), Range::Day);
    assert!(matches!(interpret(&ask, 503, "{}"), Msg::NoHistory));
    assert!(matches!(
        interpret(&Request::Hosts, 503, "{}"),
        Msg::Failed(_)
    ));
    assert!(matches!(interpret(&ask, 404, "{}"), Msg::NotFound(n) if n == "lorien"));
}

#[quench_test::test]
fn the_forecast_is_asked_for_and_read_back() {
    use gauge_core::DiskForecast;
    assert_eq!(
        url_for("/gauge", &Request::Forecast).as_deref(),
        Some("/gauge/api/v1/forecast")
    );
    let body = json::to_string(&vec![DiskForecast {
        host: "lorien".into(),
        mount: "/".into(),
        size: 10.0,
        used: 5.0,
        per_day: Some(1.0),
        days_left: Some(5.0),
    }]);
    let Msg::Forecast(list) = interpret(&Request::Forecast, 200, &body) else {
        panic!("not a forecast")
    };
    assert_eq!(list[0].days_left, Some(5.0));
    assert!(matches!(
        interpret(&Request::Forecast, 503, "{}"),
        Msg::NoHistory
    ));
}

#[quench_test::test]
fn the_alert_log_is_asked_for_and_read_back() {
    use gauge_core::AlertEvent;
    assert_eq!(
        url_for("/gauge", &Request::AlertLog).as_deref(),
        Some("/gauge/api/v1/alerts/history")
    );
    let body = json::to_string(&vec![AlertEvent {
        host: "lorien".into(),
        key: "hot:k10temp".into(),
        kind: "hot".into(),
        detail: "k10temp 95 °C".into(),
        severe: true,
        opened: 1.0,
        closed: None,
    }]);
    let Msg::AlertLog(list) = interpret(&Request::AlertLog, 200, &body) else {
        panic!("not a log")
    };
    assert_eq!(list[0].key, "hot:k10temp");
    assert!(list[0].closed.is_none());
    assert!(matches!(
        interpret(&Request::AlertLog, 503, "{}"),
        Msg::NoHistory
    ));
}
