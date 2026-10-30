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
