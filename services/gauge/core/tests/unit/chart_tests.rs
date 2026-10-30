use gauge_core::chart::{Series, data_uri, line_svg, points};

#[quench_test::test]
fn a_gap_breaks_the_line() {
    let values = [
        Some(10.0),
        Some(20.0),
        None,
        Some(30.0),
        Some(40.0),
        Some(50.0),
    ];
    let runs = points(&values, 100.0, 6);
    assert_eq!(runs.len(), 2, "{runs:?}");
    assert_eq!(runs[0].split(' ').count(), 2);
    assert_eq!(runs[1].split(' ').count(), 3);
}

#[quench_test::test]
fn a_lone_sample_draws_nothing_and_the_newest_is_at_the_right_edge() {
    assert!(points(&[Some(1.0)], 1.0, 10).is_empty());
    let runs = points(&[Some(0.0), Some(100.0)], 100.0, 10);
    assert!(runs[0].ends_with(",1.0"), "100 % is at the top: {runs:?}");
    assert!(
        runs[0].contains("300.0,"),
        "the newest sample is at x = 300: {runs:?}"
    );
    assert!(
        runs[0].starts_with("266.7,"),
        "older ones are to the left: {runs:?}"
    );
}

#[quench_test::test]
fn values_are_clamped_to_the_scale() {
    let runs = points(&[Some(500.0), Some(-5.0)], 100.0, 2);
    assert_eq!(runs[0], "0.0,1.0 300.0,63.0");
}

#[quench_test::test]
fn the_image_has_grid_lines_an_area_under_the_first_series_and_a_line_per_run() {
    let a = [Some(10.0), Some(20.0), Some(30.0)];
    let b = [Some(5.0), Some(6.0), Some(7.0)];
    let svg = line_svg(
        &[
            Series {
                colour: 1,
                values: &a,
            },
            Series {
                colour: 2,
                values: &b,
            },
        ],
        100.0,
        3,
    );
    assert!(svg.starts_with("<svg") && svg.ends_with("</svg>"));
    assert_eq!(svg.matches("class='g'").count(), 3);
    assert_eq!(
        svg.matches("<polygon").count(),
        1,
        "only the first series has an area"
    );
    assert_eq!(svg.matches("<polyline").count(), 2);
    assert!(svg.contains("l1") && svg.contains("l2"));
}

#[quench_test::test]
fn an_empty_scale_does_not_divide_by_zero() {
    let a = [Some(0.0), Some(0.0)];
    let svg = line_svg(
        &[Series {
            colour: 1,
            values: &a,
        }],
        0.0,
        2,
    );
    assert!(!svg.contains("NaN") && !svg.contains("inf"), "{svg}");
}

#[quench_test::test]
fn the_data_uri_has_nothing_a_url_would_break_on() {
    let uri = data_uri("<svg viewBox='0 0 1 1'/>");
    assert!(uri.starts_with("data:image/svg+xml,"));
    assert!(
        !uri.contains(' ') && !uri.contains('<') && !uri.contains('\''),
        "{uri}"
    );
}
