use gauge_core::forecast::fit;

const GIB: f64 = 1024.0 * 1024.0 * 1024.0;

/// `hours` hourly samples growing `gib_per_day` a day from `start` GiB.
fn growing(hours: u32, start: f64, gib_per_day: f64) -> Vec<(f64, f64)> {
    (0..hours)
        .map(|h| {
            let t = f64::from(h) * 3600.0;
            (t, (start + gib_per_day * f64::from(h) / 24.0) * GIB)
        })
        .collect()
}

#[quench_test::test]
fn a_steadily_growing_disk_gets_a_date() {
    let f = fit(&growing(72, 100.0, 10.0), 200.0 * GIB).unwrap();
    assert!((f.per_day / GIB - 10.0).abs() < 1e-6);
    // The last of 72 hourly samples is hour 71: 100 + 10 * 71 / 24 = 129.6 GiB used, 70.4 left at 10 a day.
    let left = f.days_left.unwrap();
    assert!((left - 7.04).abs() < 0.01, "{left}");
}

#[quench_test::test]
fn a_level_or_shrinking_disk_has_a_rate_but_no_date() {
    let level = fit(&growing(48, 100.0, 0.0), 200.0 * GIB).unwrap();
    assert!(level.days_left.is_none());
    let emptying = fit(&growing(48, 100.0, -5.0), 200.0 * GIB).unwrap();
    assert!(emptying.per_day < 0.0 && emptying.days_left.is_none());
}

#[quench_test::test]
fn too_little_history_gives_nothing() {
    assert!(
        fit(&growing(10, 100.0, 10.0), 200.0 * GIB).is_none(),
        "ten samples"
    );
    assert!(fit(&[], 1.0).is_none());
    let crowded: Vec<(f64, f64)> = (0..40).map(|i| (f64::from(i), 1.0)).collect();
    assert!(
        fit(&crowded, 10.0).is_none(),
        "plenty of samples, but an hour apart in total"
    );
}

#[quench_test::test]
fn a_date_further_off_than_ten_years_is_not_given() {
    let f = fit(&growing(48, 1.0, 0.01), 1.0e6 * GIB).unwrap();
    assert!(f.days_left.is_none());
}
