use forge_tui::{Rgb, Theme, Tone};
use gauge_cli::state::Data;
use gauge_cli::tiles::{TILE_ROWS, Tile, fleet_tiles, host_tiles, row, spark};
use gauge_core::{HostDetail, Point, View};

fn plain(line: &forge_tui::Line) -> String {
    line.plain()
}

const fn point(cpu: f64) -> Point {
    Point {
        cpu: Some(cpu),
        ..Point::gap(0.0)
    }
}

#[quench_test::test]
fn a_sparkline_climbs_with_its_values() {
    let theme = Theme::dark();
    let series: Vec<Option<f64>> = [0.0, 50.0, 100.0].into_iter().map(Some).collect();
    let line = spark(&theme, &series, 3, 100.0, Rgb(0, 0, 0));
    assert_eq!(plain(&line), "▁▅█");
}

#[quench_test::test]
fn a_short_series_sits_at_the_right_edge_and_gaps_are_blank() {
    let theme = Theme::dark();
    let series = vec![Some(100.0), None, Some(100.0)];
    let line = spark(&theme, &series, 6, 100.0, Rgb(0, 0, 0));
    assert_eq!(plain(&line), "   █ █");
}

#[quench_test::test]
fn a_long_series_is_averaged_down_to_the_width() {
    let theme = Theme::dark();
    let series: Vec<Option<f64>> = (0..100).map(|n| Some(f64::from(n))).collect();
    let line = spark(&theme, &series, 10, 100.0, Rgb(0, 0, 0));
    assert_eq!(plain(&line).chars().count(), 10);
    assert!(plain(&line).starts_with('▁') && plain(&line).ends_with('█'));
}

#[quench_test::test]
fn no_history_says_so() {
    let theme = Theme::dark();
    assert_eq!(
        plain(&spark(&theme, &[], 10, 100.0, Rgb(0, 0, 0))),
        "no history"
    );
    assert_eq!(
        plain(&spark(&theme, &[None, None], 10, 100.0, Rgb(0, 0, 0))),
        "no history"
    );
}

#[quench_test::test]
fn a_tile_row_is_four_lines_that_never_exceed_the_width() {
    let theme = Theme::dark();
    let tile = |label: &str| Tile {
        label: label.to_string(),
        value: "93%".to_string(),
        sub: "load 1.50".to_string(),
        bar: Some(93.0),
        tone: Tone::Error,
        series: vec![Some(10.0), Some(90.0)],
        scale: 100.0,
    };
    let tiles = [
        tile("CPU"),
        tile("Memory"),
        tile("GPU"),
        tile("Disk"),
        tile("Network"),
    ];
    for width in [10, 30, 60, 100, 120] {
        let lines = row(&theme, &tiles, width);
        assert_eq!(lines.len(), TILE_ROWS);
        for l in &lines {
            assert!(l.width() <= width.max(8), "{width}: {}", l.width());
        }
    }
    let wide = row(&theme, &tiles, 120);
    assert!(plain(&wide[0]).contains("Network"), "all five fit");
    let narrow = row(&theme, &tiles, 45);
    assert!(plain(&narrow[0]).contains("CPU"));
    assert!(
        !plain(&narrow[0]).contains("Network"),
        "the last tiles are dropped first"
    );
}

#[quench_test::test]
fn a_bar_moves_in_eighths_of_a_cell() {
    // Rendered through a tile: 10 cells, 52% is 5 whole blocks and a 2/8 partial (41.6 rounds to 42 eighths).
    let theme = Theme::dark();
    let tile = Tile {
        label: "x".into(),
        value: "52%".into(),
        sub: String::new(),
        bar: Some(52.0),
        tone: Tone::Success,
        series: Vec::new(),
        scale: 100.0,
    };
    let lines = row(&theme, &[tile], 12);
    let bar = plain(&lines[2]);
    assert!(bar.contains("█████▎░░░░"), "{bar:?}");
}

#[quench_test::test]
fn host_tiles_use_the_live_history_of_the_open_host() {
    let mut card = super::state_tests::card("a", true);
    card.host.view = Some(View {
        cpu_pct: Some(50.0),
        ..View::default()
    });
    card.cpu = vec![Some(1.0)];
    let detail = HostDetail {
        host: card.host.clone(),
        history: vec![point(10.0), point(20.0)],
    };
    let with = host_tiles(&card, Some(&detail));
    assert_eq!(with[0].series, vec![Some(10.0), Some(20.0)]);
    let without = host_tiles(&card, None);
    assert_eq!(
        without[0].series,
        vec![Some(1.0)],
        "the card's own history until the detail arrives"
    );
    let other = HostDetail {
        host: super::state_tests::card("b", true).host,
        history: vec![point(99.0)],
    };
    assert_eq!(
        host_tiles(&card, Some(&other))[0].series,
        vec![Some(1.0)],
        "another host's history is ignored"
    );
}

#[quench_test::test]
fn fleet_tiles_count_hosts_alerts_and_average_the_cpu() {
    let mut a = super::state_tests::card("a", true);
    a.host.view = Some(View {
        cpu_pct: Some(20.0),
        ..View::default()
    });
    a.cpu = vec![Some(10.0), Some(20.0)];
    let mut b = super::state_tests::card("b", true);
    b.host.view = Some(View {
        cpu_pct: Some(60.0),
        ..View::default()
    });
    b.cpu = vec![Some(30.0)];
    let down = super::state_tests::card("c", false);
    let tiles = fleet_tiles(&Data {
        hosts: vec![a, b, down],
        ..Data::default()
    });
    assert_eq!(tiles[0].value, "2/3 up");
    assert_eq!(tiles[0].tone, Tone::Warning);
    assert_eq!(tiles[1].value, "40%");
    assert_eq!(
        tiles[1].series,
        vec![Some(10.0), Some(25.0)],
        "aligned at the newest end"
    );
    let alerts = tiles.last().unwrap();
    assert_eq!(alerts.label, "Alerts");
    assert_eq!(alerts.value, "1", "the down host");
    assert_eq!(alerts.tone, Tone::Error);
}
