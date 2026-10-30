use gauge_core::format::{Level, ago, bytes, duration, pct, rate};

#[quench_test::test]
fn bytes_keep_three_significant_figures() {
    assert_eq!(bytes(0.0), "0 B");
    assert_eq!(bytes(512.0), "512 B");
    assert_eq!(bytes(1536.0), "1.50 KiB");
    assert_eq!(bytes(15.0 * 1024.0 * 1024.0), "15.0 MiB");
    assert_eq!(bytes(250.0 * 1024.0 * 1024.0 * 1024.0), "250 GiB");
    assert_eq!(bytes(-5.0), "0 B");
    assert_eq!(rate(2048.0), "2.00 KiB/s");
}

#[quench_test::test]
fn times_are_the_two_biggest_units() {
    assert_eq!(pct(41.6), "42%");
    assert_eq!(duration(59.0), "0m");
    assert_eq!(duration(3_900.0), "1h 5m");
    assert_eq!(duration(90_000.0), "1d 1h");
    assert_eq!(ago(30.0), "just now");
    assert_eq!(ago(7_320.0), "2h 2m ago");
}

#[quench_test::test]
fn a_bar_warns_at_three_quarters_and_is_critical_at_nine_tenths() {
    assert_eq!(Level::of(10.0), Level::Ok);
    assert_eq!(Level::of(74.9), Level::Ok);
    assert_eq!(Level::of(75.0), Level::Warn);
    assert_eq!(Level::of(90.0), Level::Crit);
    assert_eq!(Level::Crit.class(), "gg-crit");
}
