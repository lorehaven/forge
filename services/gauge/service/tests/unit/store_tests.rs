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

#[quench_test::test]
fn every_point_drawn_is_also_sent_to_the_tap() {
    use gauge_service::domain::alert_log::AlertChange;
    use gauge_service::domain::config::{Kind, Target};
    use gauge_service::domain::store::{Note, Store};
    let (tap, mut notes) = tokio::sync::mpsc::unbounded_channel();
    let store = Store::new(&[Target {
        name: "h".into(),
        kind: Kind::Node,
        url: "u".into(),
        also: None,
    }])
    .with_tap(tap);
    store.record("h", Ok(reading(100.0, 100.0, 100.0, 0.0)));
    assert!(
        notes.try_recv().is_err(),
        "the first scrape has no rates, so no point"
    );
    store.record("h", Ok(reading(110.0, 105.0, 110.0, 1000.0)));
    let Ok(Note::Tick(tick)) = notes.try_recv() else {
        panic!("a point")
    };
    assert_eq!(tick.host, "h");
    assert!((tick.point.t - 110.0).abs() < 1e-9);
    assert!(notes.try_recv().is_err());
    store.record("h", Err("down".into()));
    let Ok(Note::Alert(AlertChange::Opened { alert, .. })) = notes.try_recv() else {
        panic!("an outage is an alert")
    };
    assert_eq!((alert.key.as_str(), alert.severe), ("down", true));
    store.record("h", Err("still down".into()));
    assert!(
        notes.try_recv().is_err(),
        "once, not on every failed scrape"
    );
}

#[quench_test::test]
fn an_alert_opens_once_and_closes_when_it_is_gone_and_a_dead_host_closes_with_it() {
    use gauge_service::domain::alert_log::AlertChange;
    use gauge_service::domain::config::{Kind, Target};
    use gauge_service::domain::model::Temp;
    use gauge_service::domain::store::{Note, Store};
    let (tap, mut notes) = tokio::sync::mpsc::unbounded_channel();
    let store = Store::new(&[Target {
        name: "h".into(),
        kind: Kind::Node,
        url: "u".into(),
        also: None,
    }])
    .with_tap(tap);
    let hot = |at: f64, celsius: f64| Reading {
        temps: vec![Temp {
            label: "k10temp".into(),
            celsius,
        }],
        ..reading(at, at, at, 0.0)
    };
    let drain = |notes: &mut tokio::sync::mpsc::UnboundedReceiver<Note>| {
        let mut out = Vec::new();
        while let Ok(n) = notes.try_recv() {
            if let Note::Alert(c) = n {
                out.push(c);
            }
        }
        out
    };
    store.record("h", Ok(hot(100.0, 95.0)));
    let first = drain(&mut notes);
    assert!(
        matches!(&first[..], [AlertChange::Opened { alert, .. }] if alert.key == "hot:k10temp")
    );
    store.record("h", Ok(hot(110.0, 97.0)));
    assert!(drain(&mut notes).is_empty(), "hotter is the same alert");
    store.record("h", Ok(hot(120.0, 60.0)));
    let closed = drain(&mut notes);
    assert!(
        matches!(&closed[..], [AlertChange::Closed { key, at, .. }] if key == "hot:k10temp" && (*at - 120.0).abs() < 1e-9)
    );
}

#[quench_test::test]
fn a_disk_the_forecast_fills_soon_raises_an_alert_on_the_next_scrape() {
    use gauge_core::DiskForecast;
    use gauge_service::domain::config::{Kind, Target};
    use gauge_service::domain::model::Filesystem;
    use gauge_service::domain::store::{ForecastCell, Store};
    let cell = ForecastCell::default();
    let store = Store::new(&[Target {
        name: "h".into(),
        kind: Kind::Node,
        url: "u".into(),
        also: None,
    }])
    .with_forecast(cell.clone());
    let with_disk = |at: f64| Reading {
        filesystems: vec![Filesystem {
            device: "/dev/sda1".into(),
            mount: Some("/data".into()),
            size: 100.0,
            avail: 50.0,
        }],
        ..reading(at, at, at, 0.0)
    };
    store.record("h", Ok(with_disk(100.0)));
    assert!(store.get("h").unwrap().view.unwrap().alerts.is_empty());
    *cell.lock().unwrap() = vec![DiskForecast {
        host: "h".into(),
        mount: "/data".into(),
        size: 100.0,
        used: 50.0,
        per_day: Some(10.0),
        days_left: Some(5.0),
    }];
    store.record("h", Ok(with_disk(110.0)));
    let alerts = store.get("h").unwrap().view.unwrap().alerts;
    assert_eq!(alerts.len(), 1);
    assert_eq!(alerts[0].key, "fills_soon:/data");
    assert!(
        !alerts[0].severe,
        "five days is a warning, under three is severe"
    );
}
