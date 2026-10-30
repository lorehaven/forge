use gauge_core::model::{Cpu, Reading};

fn reading(at: f64, busy: f64, total: f64, rx: f64) -> Reading {
    Reading {
        at,
        cpu: Cpu {
            busy_secs: busy,
            total_secs: total,
        },
        nics: vec![("eth0".into(), rx, 0.0)],
        ..Reading::default()
    }
}

#[quench_test::test]
fn an_outage_breaks_the_history_and_forgets_the_counters() {
    use gauge_service::domain::config::{Kind, Target};
    use gauge_service::domain::store::Store;
    let store = Store::new(&[Target {
        name: "h".into(),
        kind: Kind::Node,
        url: "u".into(),
        also: None,
    }]);
    for (at, rx) in [(100.0, 0.0), (110.0, 1000.0)] {
        store.record("h", Ok(reading(at, at, at, rx)));
    }
    assert_eq!(store.get("h").unwrap().history.len(), 1);

    store.record("h", Err("down".into()));
    store.record("h", Err("still down".into()));
    let host = store.get("h").unwrap();
    assert!(!host.up());
    assert!(host.view.is_some(), "the last view is kept for display");
    assert_eq!(host.history.len(), 2, "one gap marker per outage");
    assert!(host.history.back().unwrap().is_gap());

    // The first reading after the outage has nothing to compare against: no rates, no point.
    store.record("h", Ok(reading(5000.0, 5000.0, 5000.0, 9e9)));
    let host = store.get("h").unwrap();
    assert!(host.up());
    assert!(host.view.as_ref().unwrap().cpu_pct.is_none());
    assert_eq!(host.history.len(), 2);
}
