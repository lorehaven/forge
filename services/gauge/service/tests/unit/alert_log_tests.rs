use gauge_core::Alert;
use gauge_service::domain::alert_log::{AlertChange, AlertLog};

fn alert(key: &str, detail: &str) -> Alert {
    Alert {
        key: key.into(),
        kind: key.split(':').next().unwrap().into(),
        detail: detail.into(),
        severe: false,
    }
}

fn opened(c: &AlertChange) -> Option<&str> {
    match c {
        AlertChange::Opened { alert, .. } => Some(&alert.key),
        AlertChange::Closed { .. } => None,
    }
}

fn closed(c: &AlertChange) -> Option<&str> {
    match c {
        AlertChange::Closed { key, .. } => Some(key),
        AlertChange::Opened { .. } => None,
    }
}

#[quench_test::test]
fn an_alert_opens_once_however_long_it_lasts_and_closes_when_it_is_gone() {
    let mut log = AlertLog::default();
    let a = [alert("hot:k10temp", "95")];
    let first = log.seen("h", 1.0, &a);
    assert_eq!(
        first.iter().filter_map(opened).collect::<Vec<_>>(),
        ["hot:k10temp"]
    );
    assert!(
        log.seen("h", 2.0, &[alert("hot:k10temp", "97")]).is_empty(),
        "worse is not new"
    );
    let end = log.seen("h", 3.0, &[]);
    assert_eq!(
        end.iter().filter_map(closed).collect::<Vec<_>>(),
        ["hot:k10temp"]
    );
    assert!(matches!(end[0], AlertChange::Closed { at, .. } if (at - 3.0).abs() < 1e-9));
    assert!(log.seen("h", 4.0, &[]).is_empty());
}

#[quench_test::test]
fn hosts_and_keys_are_tracked_apart() {
    let mut log = AlertLog::default();
    log.seen("a", 1.0, &[alert("hot:x", "1"), alert("oom", "1")]);
    let b = log.seen("b", 1.0, &[alert("hot:x", "1")]);
    assert_eq!(b.len(), 1, "the same key on another host is its own alert");
    let a = log.seen("a", 2.0, &[alert("oom", "1"), alert("swapping", "1")]);
    assert_eq!(a.iter().filter_map(closed).collect::<Vec<_>>(), ["hot:x"]);
    assert_eq!(
        a.iter().filter_map(opened).collect::<Vec<_>>(),
        ["swapping"]
    );
}

#[quench_test::test]
fn a_silent_host_is_one_alert_and_its_other_alerts_wait_for_it() {
    let mut log = AlertLog::default();
    log.seen("h", 1.0, &[alert("hot:x", "1")]);
    let down = log.silent("h", 2.0, "connection refused");
    assert_eq!(down.iter().filter_map(opened).collect::<Vec<_>>(), ["down"]);
    assert!(log.silent("h", 3.0, "still refused").is_empty());
    // It comes back and the heat is still there: the outage ends, the heat was never interrupted.
    let back = log.seen("h", 4.0, &[alert("hot:x", "1")]);
    assert_eq!(back.iter().filter_map(closed).collect::<Vec<_>>(), ["down"]);
    assert!(back.iter().find_map(opened).is_none());
}
