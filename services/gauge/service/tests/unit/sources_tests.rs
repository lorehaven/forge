use gauge_service::domain::config::Config;
use gauge_service::domain::prom::parse;
use gauge_service::domain::sources::{node, pcp};

#[quench_test::test]
fn node_exporter_text_becomes_a_reading() {
    let x = parse(
        "node_cpu_seconds_total{cpu=\"0\",mode=\"idle\"} 90\nnode_cpu_seconds_total{cpu=\"0\",mode=\"user\"} 10\nnode_memory_MemTotal_bytes 1000\nnode_memory_MemAvailable_bytes 250\nnode_load1 0.5\nnode_load5 0.4\nnode_load15 0.3\nnode_filesystem_size_bytes{device=\"/dev/sda1\",fstype=\"ext4\",mountpoint=\"/\"} 100\nnode_filesystem_avail_bytes{device=\"/dev/sda1\",fstype=\"ext4\",mountpoint=\"/\"} 40\nnode_filesystem_size_bytes{device=\"tmpfs\",fstype=\"tmpfs\",mountpoint=\"/run\"} 100\n",
    );
    let r = node::reading(&x, 1.0);
    assert!((r.cpu.busy_secs - 10.0).abs() < 1e-9 && (r.cpu.total_secs - 100.0).abs() < 1e-9);
    assert_eq!(r.cores.len(), 1);
    assert_eq!(r.filesystems.len(), 1);
    assert_eq!(r.load, Some([0.5, 0.4, 0.3]));
}

#[quench_test::test]
fn pcp_text_is_scaled_to_bytes_and_seconds() {
    let x = parse(
        "# PCP5 mem.physmem 60.1.0 u64 PM_INDOM_NULL discrete Kbyte\nmem_physmem 4\n# PCP5 kernel.all.cpu.user 60.0.20 u64 PM_INDOM_NULL counter millisec\nkernel_all_cpu_user 5000\n# PCP5 kernel.all.cpu.idle 60.0.21 u64 PM_INDOM_NULL counter millisec\nkernel_all_cpu_idle 15000\n",
    );
    let r = pcp::reading(&x, 1.0);
    assert!((r.mem_total.unwrap() - 4096.0).abs() < 1e-9);
    assert!((r.cpu.busy_secs - 5.0).abs() < 1e-9 && (r.cpu.total_secs - 20.0).abs() < 1e-9);
}

#[quench_test::test]
fn targets_parse_and_names_must_be_unique() {
    let ok = Config::parse("[[target]]\nname=\"a\"\nkind=\"pcp\"\nurl=\"http://h:1/\"\n").unwrap();
    assert_eq!(ok.targets[0].url, "http://h:1");
    assert_eq!(ok.interval_secs, 10);
    let twice = "[[target]]\nname=\"a\"\nkind=\"node\"\nurl=\"u\"\n[[target]]\nname=\"a\"\nkind=\"node\"\nurl=\"u\"\n";
    assert!(Config::parse(twice).is_err());
}

#[quench_test::test]
fn a_side_agent_is_optional_and_trimmed() {
    let with = Config::parse(
        "[[target]]\nname=\"a\"\nkind=\"pcp\"\nurl=\"http://h:1\"\nalso=\"http://h:2/\"\n",
    )
    .unwrap();
    assert_eq!(with.targets[0].also.as_deref(), Some("http://h:2"));
    let without = Config::parse("[[target]]\nname=\"a\"\nkind=\"node\"\nurl=\"u\"\n").unwrap();
    assert!(without.targets[0].also.is_none());
}

const OSSIRIAND: &str = include_str!("../fixtures/node_two_gpus.txt");

#[quench_test::test]
fn the_discrete_card_gets_the_discrete_chip_when_two_cards_are_present() {
    let r = node::reading(&parse(OSSIRIAND), 1.0);
    assert_eq!(r.gpus.len(), 2);
    let big = r.gpus.iter().find(|g| g.name == "card0").unwrap();
    // the hottest of its edge, junction and memory sensors
    assert_eq!(big.temp_c, Some(40.0));
    assert_eq!(big.power_w, Some(22.0));
    assert_eq!(big.power_cap_w, Some(300.0));
    assert_eq!(big.fan_rpm, Some(1074.0));
    assert_eq!(big.mclk_mhz, Some(96.0));
    assert!(big.gtt_total.is_some());
    let small = r.gpus.iter().find(|g| g.name == "card1").unwrap();
    assert_eq!(
        small.temp_c,
        Some(40.0),
        "the integrated chip belongs to the small card"
    );
    assert_eq!(small.power_cap_w, None);
    assert_eq!(small.fan_rpm, None);
}

#[quench_test::test]
fn pressure_memory_swap_disks_and_links_are_read() {
    let r = node::reading(&parse(OSSIRIAND), 1.0);
    assert!(r.psi.is_some());
    let parts = r.mem_parts.unwrap();
    assert!(parts.cached > 0.0 && parts.anon > 0.0);
    assert!(r.vm.is_some());
    let disk = r.disk_io.iter().find(|d| d.name == "nvme0n1").unwrap();
    assert!(disk.busy_secs > 0.0 && disk.reads > 0.0);
    assert!(r.cpu_freq_hz.unwrap() > 1.0e8);
    assert!(!r.links.is_empty());
    assert!(r.links.iter().all(|l| l.name != "lo"));
}

#[quench_test::test]
fn fans_skip_the_gpu_and_the_ones_that_do_not_spin() {
    let r = node::reading(&parse(OSSIRIAND), 1.0);
    assert!(r.fans.iter().all(|f| f.rpm > 0.0));
    assert!(r.fans.iter().any(|f| f.label.starts_with("x53")));
    assert!(r.fans.iter().all(|f| !f.label.starts_with("amdgpu")));
}

#[quench_test::test]
fn error_counters_that_moved_are_faults_and_zeroes_are_not() {
    let healthy = node::reading(&parse(OSSIRIAND), 1.0);
    assert!(healthy.faults.is_empty());
    let sick = parse(
        "node_btrfs_device_errors_total{device=\"sda\",type=\"corruption\",uuid=\"u\"} 3\nnode_btrfs_device_errors_total{device=\"sda\",type=\"flush\",uuid=\"u\"} 0\nnode_edac_uncorrectable_errors_total{controller=\"0\"} 1\n",
    );
    let r = node::reading(&sick, 1.0);
    let what: Vec<&str> = r.faults.iter().map(|f| f.what.as_str()).collect();
    assert_eq!(
        what,
        ["btrfs sda corruption", "ecc uncorrectable (controller 0)"]
    );
}

#[quench_test::test]
fn a_side_agent_fills_in_what_the_main_one_lacks() {
    let mut main = pcp::reading(&parse("mem_physmem 1\n"), 1.0);
    main.merge(node::reading(&parse(OSSIRIAND), 1.0));
    assert!(main.psi.is_some() && main.mem_parts.is_some());
    assert_eq!(main.gpus.len(), 2);
    assert!(!main.disk_io.is_empty());
}
