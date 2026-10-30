use gauge_service::domain::prom::parse;

#[quench_test::test]
fn labels_values_and_escapes_are_read() {
    let x = parse(
        "# HELP a x\n# TYPE a gauge\na{k=\"v, \\\"q\\\"\",n=\"2\"} 1.5\nb 3 1700000000\nc{} NaN\n",
    );
    assert_eq!(x.samples.len(), 3);
    assert_eq!(x.samples[0].label("k"), Some("v, \"q\""));
    assert!((x.one("a").unwrap() - 1.5).abs() < f64::EPSILON);
    assert!((x.one("b").unwrap() - 3.0).abs() < f64::EPSILON);
    assert!(x.one("c").unwrap().is_nan());
}

#[quench_test::test]
fn pcp_units_scale_values() {
    let x = parse(
        "# PCP5 mem.physmem 60.1.0 u64 PM_INDOM_NULL discrete Kbyte\nmem_physmem{agent=\"linux\"} 2\n# PCP5 kernel.all.cpu.user 60.0.20 u64 PM_INDOM_NULL counter millisec\nkernel_all_cpu_user 5\n",
    );
    assert!((x.byte_scale("mem_physmem") - 1024.0).abs() < f64::EPSILON);
    assert!((x.second_scale("kernel_all_cpu_user") - 0.001).abs() < f64::EPSILON);
    assert!((x.byte_scale("unknown") - 1.0).abs() < f64::EPSILON);
}
