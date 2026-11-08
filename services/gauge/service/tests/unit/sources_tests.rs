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
    assert_eq!(healthy.faults, [] as [gauge_core::Fault; 0]);
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

const SMARTMON: &str = include_str!("../fixtures/node_smartmon.txt");

#[quench_test::test]
fn smartmon_drives_are_read_with_their_counters_and_nvme_has_only_its_verdict() {
    let r = node::reading(&parse(SMARTMON), 1.0);
    let names: Vec<&str> = r.disk_health.iter().map(|d| d.device.as_str()).collect();
    assert_eq!(names, ["nvme0", "sda", "sdb", "sdc", "sdd", "sde"]);
    assert!(r.disk_health.iter().all(|d| d.healthy));
    let sdd = &r.disk_health[4];
    assert_eq!(sdd.model.as_deref(), Some("TOSHIBA HDWD240"));
    assert_eq!(sdd.reallocated, Some(163.0));
    assert_eq!(sdd.pending, Some(0.0));
    assert_eq!(sdd.power_on_hours, Some(33079.0));
    assert_eq!(sdd.temp_c, Some(33.0));
    let nvme = &r.disk_health[0];
    assert_eq!(nvme.model, None);
    assert!(nvme.temp_c.is_none() && nvme.reallocated.is_none());
}

const NVME: &str = include_str!("../fixtures/node_nvme.txt");

#[quench_test::test]
fn nvme_without_nvme_metrics_takes_model_and_temperature_from_node_exporter() {
    // smartmon and node_exporter only: the `nvme_*` series of nvme_metrics.py left out.
    let text = NVME
        .lines()
        .filter(|l| !l.starts_with("nvme_"))
        .collect::<Vec<_>>()
        .join("\n");
    let r = node::reading(&parse(&text), 1.0);
    let nvme = &r.disk_health[0];
    assert_eq!(nvme.device, "nvme0");
    assert!(nvme.healthy);
    assert_eq!(nvme.model.as_deref(), Some("MSI M480 PRO 2TB"));
    // temp1 is the composite; temp2 (a hotter sensor on the controller) is not the drive's temperature.
    assert_eq!(nvme.temp_c, Some(39.85));
    assert!(nvme.power_on_hours.is_none() && nvme.wear_pct.is_none());
    // No smartmon row and no nvme_metrics for nvme1: nothing says how it is, so it is not listed.
    assert_eq!(r.disk_health.len(), 1);
}

#[quench_test::test]
fn nvme_metrics_fills_wear_spare_and_errors_and_adds_drives_smartmon_does_not_list() {
    let r = node::reading(&parse(NVME), 1.0);
    let names: Vec<&str> = r.disk_health.iter().map(|d| d.device.as_str()).collect();
    assert_eq!(names, ["nvme0", "nvme1"]);

    let a = &r.disk_health[0];
    assert!(a.healthy);
    assert_eq!(a.temp_c, Some(40.0));
    assert_eq!(a.power_on_hours, Some(9123.0));
    assert_eq!(a.wear_pct, Some(3.0));
    assert_eq!(a.spare_pct, Some(100.0));
    assert_eq!(a.spare_threshold_pct, Some(10.0));
    assert_eq!(a.media_errors, Some(0.0));
    assert!(a.reallocated.is_none() && a.pending.is_none());

    // A critical warning makes the drive failing even with no smartmon verdict to go by.
    let b = &r.disk_health[1];
    assert!(!b.healthy);
    assert_eq!(b.model.as_deref(), Some("Samsung SSD 980 PRO 1TB"));
    assert_eq!(b.wear_pct, Some(84.0));
    assert_eq!(b.media_errors, Some(3.0));
}

#[quench_test::test]
fn a_sata_ssd_has_its_wear_from_the_life_left_attribute() {
    let text = r#"smartmon_device_smart_available{disk="/dev/sda",type="sat"} 1
smartmon_device_smart_healthy{disk="/dev/sda",type="sat"} 1
smartmon_device_info{device_model="Samsung SSD 870 EVO 1TB",disk="/dev/sda",type="sat"} 1
smartmon_wear_leveling_count_value{disk="/dev/sda",smart_id="177",type="sat"} 97
smartmon_wear_leveling_count_raw_value{disk="/dev/sda",smart_id="177",type="sat"} 2.100000e+01
smartmon_device_smart_available{disk="/dev/sdb",type="sat"} 1
smartmon_device_smart_healthy{disk="/dev/sdb",type="sat"} 1
smartmon_percent_lifetime_remain_value{disk="/dev/sdb",smart_id="202",type="sat"} 18
smartmon_device_smart_available{disk="/dev/sdc",type="sat"} 1
smartmon_device_smart_healthy{disk="/dev/sdc",type="sat"} 1
"#;
    let r = node::reading(&parse(text), 1.0);
    let wear: Vec<Option<f64>> = r.disk_health.iter().map(|d| d.wear_pct).collect();
    // 97 left is 3 used (not the raw 21, which is a count of erase cycles); a spinning disk has none.
    assert_eq!(wear, [Some(3.0), Some(82.0), None]);
}

#[quench_test::test]
fn an_nvme_namespace_belongs_to_its_controller() {
    assert_eq!(node::nvme_controller("nvme0n1"), "nvme0");
    assert_eq!(node::nvme_controller("nvme12n3"), "nvme12");
    assert_eq!(node::nvme_controller("nvme0"), "nvme0");
    assert_eq!(node::nvme_controller("sda"), "sda");
}

#[quench_test::test]
fn a_pcp_host_takes_its_smart_data_from_the_side_agent() {
    let mut main = pcp::reading(&parse("mem_physmem 1\n"), 1.0);
    main.merge(node::reading(&parse(SMARTMON), 1.0));
    assert_eq!(main.disk_health.len(), 6);
    assert_eq!(
        node::reading(&parse(OSSIRIAND), 1.0).disk_health,
        [] as [gauge_core::DiskHealth; 0]
    );
}
