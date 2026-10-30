use gauge_core::Range;
use gauge_service::domain::history::{Rollup, grid};
use gauge_service::domain::store::Point;

const fn at(t: f64, cpu: f64, mem: Option<f64>) -> Point {
    Point {
        cpu: Some(cpu),
        mem,
        rx: Some(0.0),
        ..Point::gap(t)
    }
}

#[quench_test::test]
fn a_minute_is_handed_back_when_the_next_one_starts_with_its_mean_and_its_peak() {
    let mut r = Rollup::default();
    assert!(r.push(&at(120.0, 10.0, Some(50.0))).is_none());
    assert!(r.push(&at(130.0, 30.0, None)).is_none());
    assert!(r.push(&at(179.0, 20.0, Some(70.0))).is_none());
    let done = r
        .push(&at(180.0, 99.0, None))
        .expect("the first minute is over");
    assert_eq!(done.minute, 120);
    assert_eq!(done.scrapes, 3);
    assert!((done.mean[0].unwrap() - 20.0).abs() < 1e-9);
    assert_eq!(done.max[0], Some(30.0));
    assert!(
        (done.mean[1].unwrap() - 60.0).abs() < 1e-9,
        "the mean is over the scrapes that reported it"
    );
    assert_eq!(done.max[1], Some(70.0));
    assert_eq!(done.mean[6], None, "no GPU, no value");
}

#[quench_test::test]
fn a_gap_adds_nothing_and_a_quiet_host_is_flushed_by_the_clock() {
    let mut r = Rollup::default();
    assert!(r.push(&Point::gap(10.0)).is_none());
    r.push(&at(70.0, 5.0, None));
    assert!(r.close_before(100.0).is_none(), "its minute is still going");
    let done = r.close_before(130.0).expect("a minute later it is over");
    assert_eq!((done.minute, done.scrapes), (60, 1));
    assert!(
        r.close_before(500.0).is_none(),
        "and it is not handed back twice"
    );
}

#[quench_test::test]
fn the_grid_has_a_slot_per_step_with_gaps_where_nothing_was_stored() {
    let mut v = [None; 13];
    v[0] = Some(7.0);
    let found = [(600.0, v), (1500.0, v)];
    let points = grid(300.0, 300.0, 6, &found);
    assert_eq!(points.len(), 6);
    let cpu: Vec<Option<f64>> = points.iter().map(|p| p.cpu).collect();
    assert_eq!(cpu, [None, Some(7.0), None, None, Some(7.0), None]);
    assert!((points[0].t - 300.0).abs() < 1e-9 && (points[5].t - 1800.0).abs() < 1e-9);
}

#[quench_test::test]
fn the_ranges_are_named_by_what_they_cover_and_have_a_few_hundred_points() {
    for (name, range, points) in [
        ("24h", Range::Day, 288),
        ("7d", Range::Week, 336),
        ("30d", Range::Month, 360),
    ] {
        assert_eq!(Range::parse(name), Some(range));
        assert_eq!(range.as_str(), name);
        assert_eq!(range.window_secs() / range.step_secs(), points);
    }
    assert_eq!(Range::parse("1h"), Some(Range::Live));
    assert_eq!(Range::parse("yesterday"), None);
}
