use gauge_core::model::{
    Cpu, DiskIo, Fault, Filesystem, Psi, Reading, Temp, View, Vm, is_real_nic, is_whole_disk,
};

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
fn rates_come_from_two_readings() {
    let a = reading(100.0, 10.0, 100.0, 1000.0);
    let b = reading(110.0, 15.0, 120.0, 3000.0);
    let view = View::from_readings(Some(&a), &b);
    assert!((view.cpu_pct.unwrap() - 25.0).abs() < 1e-9);
    assert!((view.nics[0].a_per_sec - 200.0).abs() < 1e-9);
    assert!(View::from_readings(None, &b).cpu_pct.is_none());
}

#[quench_test::test]
fn a_reset_counter_is_not_a_rate() {
    let a = reading(100.0, 10.0, 100.0, 5000.0);
    let b = reading(110.0, 11.0, 110.0, 10.0);
    assert!(View::from_readings(Some(&a), &b).nics.is_empty());
}

#[quench_test::test]
fn disks_and_interfaces_are_filtered() {
    for disk in ["sda", "nvme0n1", "vdb", "mmcblk0"] {
        assert!(is_whole_disk(disk), "{disk}");
    }
    for part in ["sda1", "nvme0n1p2", "loop0", "dm-0", "zram0", "mmcblk0p1"] {
        assert!(!is_whole_disk(part), "{part}");
    }
    assert!(is_real_nic("enp15s0") && is_real_nic("wg0"));
    assert!(!is_real_nic("veth1a") && !is_real_nic("cni0") && !is_real_nic("lo"));
}

fn with_stalls(at: f64, mem_some: f64, kills: f64) -> Reading {
    Reading {
        at,
        psi: Some(Psi {
            mem_some,
            ..Psi::default()
        }),
        vm: Some(Vm {
            oom_kills: kills,
            swap_out: at * 1000.0,
            ..Vm::default()
        }),
        disk_io: vec![DiskIo {
            name: "sda".into(),
            busy_secs: at / 2.0,
            reads: at * 10.0,
            writes: 0.0,
            read_secs: at / 100.0,
            ..DiskIo::default()
        }],
        ..Reading::default()
    }
}

#[quench_test::test]
fn pressure_and_disk_load_are_shares_of_the_interval() {
    let v = View::from_readings(
        Some(&with_stalls(10.0, 0.0, 0.0)),
        &with_stalls(20.0, 2.0, 0.0),
    );
    assert!((v.psi.unwrap().mem - 20.0).abs() < 1e-9);
    let d = &v.disk_stats[0];
    assert!((d.util_pct - 50.0).abs() < 1e-9);
    assert!((d.iops - 10.0).abs() < 1e-9);
    assert!(
        (d.read_ms.unwrap() - 1.0).abs() < 1e-9,
        "0.1 s over 100 reads"
    );
    assert_eq!(d.write_ms, None);
    assert!((v.swap_out_per_sec.unwrap() - 1000.0 * 4096.0).abs() < 1e-6);
}

#[quench_test::test]
fn an_oom_kill_since_the_last_scrape_is_a_severe_alert_and_an_old_one_is_not() {
    let fresh = View::from_readings(
        Some(&with_stalls(10.0, 0.0, 1.0)),
        &with_stalls(20.0, 0.0, 2.0),
    );
    assert_eq!(fresh.severe_alerts(), 1);
    assert_eq!(fresh.alerts[0].kind, "oom");
    let old = View::from_readings(
        Some(&with_stalls(10.0, 0.0, 2.0)),
        &with_stalls(20.0, 0.0, 2.0),
    );
    assert!(old.alerts.iter().all(|a| a.kind != "oom"));
    assert_eq!(old.oom_kills, Some(2.0));
}

#[quench_test::test]
fn full_disks_hot_chips_and_faults_raise_alerts() {
    let cur = Reading {
        at: 2.0,
        filesystems: vec![Filesystem {
            device: "/dev/sda1".into(),
            mount: Some("/data".into()),
            size: 100.0,
            avail: 4.0,
        }],
        temps: vec![Temp {
            label: "amdgpu".into(),
            celsius: 95.0,
        }],
        faults: vec![Fault {
            what: "btrfs sda flush".into(),
            count: 1.0,
        }],
        ..Reading::default()
    };
    let kinds: Vec<(String, bool)> = View::from_readings(None, &cur)
        .alerts
        .into_iter()
        .map(|a| (a.kind, a.severe))
        .collect();
    assert_eq!(
        kinds,
        [
            ("fault".into(), true),
            ("disk_full".into(), false),
            ("hot".into(), true)
        ]
    );
}

#[quench_test::test]
fn a_counter_that_went_backwards_has_no_rate() {
    let v = View::from_readings(
        Some(&with_stalls(20.0, 5.0, 0.0)),
        &with_stalls(30.0, 1.0, 0.0),
    );
    assert!(v.psi.is_none());
}

#[quench_test::test]
fn an_alert_keeps_its_key_while_its_wording_changes() {
    let at = |avail: f64| Reading {
        at: 2.0,
        filesystems: vec![Filesystem {
            device: "/dev/sda1".into(),
            mount: Some("/data".into()),
            size: 100.0,
            avail,
        }],
        temps: vec![Temp {
            label: "k10temp".into(),
            celsius: 95.0,
        }],
        ..Reading::default()
    };
    let (a, b) = (
        View::from_readings(None, &at(7.0)),
        View::from_readings(None, &at(4.0)),
    );
    let keys = |v: &View| v.alerts.iter().map(|x| x.key.clone()).collect::<Vec<_>>();
    assert_eq!(keys(&a), ["disk_full:/data", "hot:k10temp"]);
    assert_eq!(keys(&a), keys(&b));
    assert_ne!(
        a.alerts[0].detail, b.alerts[0].detail,
        "the wording follows the number"
    );
}

#[quench_test::test]
fn a_disk_the_forecast_fills_soon_is_an_alert_and_the_nearer_the_worse() {
    use gauge_core::DiskForecast;
    use gauge_core::model::fill_alerts;
    let d = |host: &str, mount: &str, days: Option<f64>| DiskForecast {
        host: host.into(),
        mount: mount.into(),
        size: 1.0,
        used: 0.5,
        per_day: Some(1.0),
        days_left: days,
    };
    let list = [
        d("lorien", "/", Some(20.0)),
        d("lorien", "/data", Some(9.0)),
        d("lorien", "/scratch", Some(2.0)),
        d("lorien", "/new", Some(0.4)),
        d("lorien", "/level", None),
        d("nogrod", "/", Some(1.0)),
    ];
    let alerts = fill_alerts("lorien", &list);
    let seen: Vec<(&str, bool)> = alerts.iter().map(|a| (a.key.as_str(), a.severe)).collect();
    assert_eq!(
        seen,
        [
            ("fills_soon:/data", false),
            ("fills_soon:/scratch", true),
            ("fills_soon:/new", true)
        ]
    );
    assert_eq!(alerts[0].detail, "/data · 9 d");
    assert_eq!(alerts[2].detail, "/new · <1 d");
}
