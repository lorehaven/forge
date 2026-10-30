use gauge_core::model::{Cpu, Reading, View, is_real_nic, is_whole_disk};

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
