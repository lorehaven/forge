use mail_client::date::{days_from_civil, format, parse};

#[quench_test::test]
fn a_standard_date_parses_to_unix_seconds() {
    assert_eq!(parse("Thu, 01 Jan 1970 00:00:00 +0000"), Some(0));
    assert_eq!(
        parse("Tue, 29 Sep 2026 12:00:00 +0000"),
        Some(1_790_683_200)
    );
}

#[quench_test::test]
fn zones_shift_the_instant() {
    let utc = parse("Tue, 29 Sep 2026 12:00:00 +0000").unwrap();
    assert_eq!(parse("Tue, 29 Sep 2026 14:00:00 +0200"), Some(utc));
    assert_eq!(parse("Tue, 29 Sep 2026 07:30:00 -0430"), Some(utc));
    assert_eq!(parse("29 Sep 2026 12:00:00 GMT"), Some(utc));
    assert_eq!(parse("29 Sep 2026 12:00:00 UT"), Some(utc));
    assert_eq!(parse("29 Sep 2026 07:00:00 EST"), Some(utc));
    assert_eq!(parse("29 Sep 2026 08:00:00 EDT"), Some(utc));
    assert_eq!(parse("29 Sep 2026 06:00:00 CST"), Some(utc));
    assert_eq!(parse("29 Sep 2026 07:00:00 CDT"), Some(utc));
    assert_eq!(parse("29 Sep 2026 05:00:00 MST"), Some(utc));
    assert_eq!(parse("29 Sep 2026 06:00:00 MDT"), Some(utc));
    assert_eq!(parse("29 Sep 2026 04:00:00 PST"), Some(utc));
    assert_eq!(parse("29 Sep 2026 05:00:00 PDT"), Some(utc));
    assert_eq!(parse("29 Sep 2026 13:00:00 CET"), Some(utc));
    assert_eq!(parse("29 Sep 2026 14:00:00 CEST"), Some(utc));
}

#[quench_test::test]
fn sloppy_variants_are_read() {
    let base = parse("29 Sep 2026 12:00:00 +0000").unwrap();
    assert_eq!(parse("Tue, 29 Sep 2026 12:00 +0000"), Some(base));
    assert_eq!(parse("29 Sep 26 12:00:00 +0000"), Some(base));
    assert_eq!(
        parse("29 Sep 99 12:00:00 +0000"),
        parse("29 Sep 1999 12:00:00 +0000")
    );
    assert_eq!(parse("Tue, 29 Sep 2026 12:00:00 +0000 (UTC)"), Some(base));
    assert_eq!(parse("29 September 2026 12:00:00 +0000"), Some(base));
    assert_eq!(
        parse("29 Sep 2026"),
        Some(parse("29 Sep 2026 00:00:00").unwrap())
    );
    assert_eq!(parse("29 Sep 2026 12:00:00"), Some(base));
}

#[quench_test::test]
fn nonsense_is_none() {
    for bad in [
        "",
        "yesterday",
        "32 Jan 2026 00:00:00",
        "1 Foo 2026 00:00:00",
        "1 Jan 2026 25:00:00",
        "1 Jan 2026 10:61:00",
        "x Jan 2026",
    ] {
        assert_eq!(parse(bad), None, "{bad}");
    }
}

#[quench_test::test]
fn format_round_trips() {
    for t in [0, 1_790_683_200, 951_782_400, -86_400, 4_102_444_800] {
        assert_eq!(parse(&format(t)), Some(t), "{t}");
    }
    assert_eq!(format(1_790_683_200), "Tue, 29 Sep 2026 12:00:00 +0000");
    assert_eq!(format(0), "Thu, 1 Jan 1970 00:00:00 +0000");
}

#[quench_test::test]
fn civil_days_are_right_around_leap_years() {
    assert_eq!(days_from_civil(1970, 1, 1), 0);
    assert_eq!(
        days_from_civil(2000, 3, 1) - days_from_civil(2000, 2, 28),
        2
    );
    assert_eq!(
        days_from_civil(1900, 3, 1) - days_from_civil(1900, 2, 28),
        1
    );
}
