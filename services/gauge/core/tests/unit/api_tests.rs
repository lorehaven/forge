use gauge_core::model::{Filesystem, Gpu, Point, Rate, Temp, View};
use gauge_core::{HostCard, HostDetail, HostSummary};
use quench_serde::json;

fn view() -> View {
    View {
        at: 1_800_000_000.0,
        hostname: Some("lorien".into()),
        uptime_secs: Some(86_400.0),
        cpu_pct: Some(12.5),
        cores_pct: vec![("cpu0".into(), 10.0), ("cpu1".into(), 15.0)],
        load: Some([0.5, 0.4, 0.3]),
        mem_total: Some(64.0e9),
        mem_used: Some(16.0e9),
        swap_total: Some(8.0e9),
        swap_used: Some(0.0),
        filesystems: vec![Filesystem {
            device: "/dev/nvme0n1p2".into(),
            mount: Some("/".into()),
            size: 1.0e12,
            avail: 4.0e11,
        }],
        disks: vec![Rate {
            name: "nvme0n1".into(),
            a_per_sec: 1.0,
            b_per_sec: 2.0,
        }],
        nics: vec![Rate {
            name: "eth0".into(),
            a_per_sec: 3.0,
            b_per_sec: 4.0,
        }],
        gpus: vec![Gpu {
            name: "RX 7900".into(),
            busy_pct: Some(40.0),
            vram_used: Some(8.0e9),
            vram_total: Some(24.0e9),
            temp_c: Some(55.0),
            ..Gpu::default()
        }],
        temps: vec![Temp {
            label: "Tctl".into(),
            celsius: 48.0,
        }],
        ..View::default()
    }
}

fn summary() -> HostSummary {
    HostSummary {
        name: "lorien".into(),
        kind: "pcp".into(),
        url: "http://lorien:44323".into(),
        up: true,
        error: None,
        last_ok: Some(1_800_000_000.0),
        view: Some(view()),
    }
}

#[quench_test::test]
fn a_host_survives_the_trip_through_json() {
    let text = json::to_string(&summary());
    let back: HostSummary = json::from_str(&text).unwrap();
    assert_eq!(back.name, "lorien");
    assert!(back.up && back.error.is_none());
    let v = back.view.unwrap();
    assert_eq!(
        v.cores_pct,
        vec![("cpu0".to_string(), 10.0), ("cpu1".to_string(), 15.0)]
    );
    assert_eq!(v.load, Some([0.5, 0.4, 0.3]));
    assert_eq!(v.gpus[0].vram_total, Some(24.0e9));
    assert_eq!(v.mem_pct(), Some(25.0));
}

#[quench_test::test]
fn a_host_that_never_answered_has_no_view() {
    let down = HostSummary {
        up: false,
        error: Some("connection refused".into()),
        last_ok: None,
        view: None,
        ..summary()
    };
    let back: HostSummary = json::from_str(&json::to_string(&down)).unwrap();
    assert!(!back.up && back.view.is_none());
    assert_eq!(back.error.as_deref(), Some("connection refused"));
}

#[quench_test::test]
fn the_detail_is_the_host_with_its_history_beside_it() {
    let detail = HostDetail {
        host: summary(),
        history: vec![
            Point::gap(1.0),
            Point {
                cpu: Some(3.0),
                ..Point::gap(2.0)
            },
        ],
    };
    let value: quench_serde::Value = json::from_str(&json::to_string(&detail)).unwrap();
    assert!(
        value.get("name").is_some(),
        "the host's fields are not nested"
    );
    assert_eq!(
        value
            .get("history")
            .and_then(quench_serde::Value::as_array)
            .map(Vec::len),
        Some(2)
    );
    let back: HostDetail = json::from_str(&json::to_string(&detail)).unwrap();
    assert_eq!(back.host.name, "lorien");
    assert!(back.history[0].is_gap() && !back.history[1].is_gap());
}

#[quench_test::test]
fn a_card_has_the_cpu_history_beside_the_host() {
    let card = HostCard {
        host: summary(),
        cpu: vec![Some(1.0), None, Some(3.0)],
    };
    let back: HostCard = json::from_str(&json::to_string(&card)).unwrap();
    assert_eq!(back.host.name, "lorien");
    assert_eq!(back.cpu, vec![Some(1.0), None, Some(3.0)]);
}
