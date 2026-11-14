use super::state_tests::card;
use forge_tui::{ColorLevel, Theme};
use gauge_cli::state::{App, Data, Screen, handle_key};
use gauge_cli::view::{current_alerts, frame, render};
use gauge_core::{Alert, AlertEvent, DiskForecast, Filesystem, Gpu, View};
use quench_cli::keys::{KeyCode, KeyEvent};

fn busy_host(name: &str) -> gauge_core::HostCard {
    let mut c = card(name, true);
    c.host.view = Some(View {
        cpu_pct: Some(93.0),
        mem_total: Some(32.0 * 1024.0 * 1024.0 * 1024.0),
        mem_used: Some(8.0 * 1024.0 * 1024.0 * 1024.0),
        load: Some([1.5, 1.0, 0.5]),
        gpus: vec![Gpu {
            name: "gfx1201".into(),
            busy_pct: Some(40.0),
            ..Gpu::default()
        }],
        filesystems: vec![Filesystem {
            device: "/dev/sda1".into(),
            mount: Some("/data".into()),
            size: 1000.0 * 1024.0 * 1024.0 * 1024.0,
            avail: 100.0 * 1024.0 * 1024.0 * 1024.0,
        }],
        alerts: vec![Alert {
            key: "cpu".into(),
            kind: "cpu".into(),
            detail: "CPU 93% for 5 minutes".into(),
            severe: false,
        }],
        ..View::default()
    });
    c
}

fn app() -> App {
    let mut app = App::new();
    app.apply(
        Ok(Data {
            hosts: vec![busy_host("ossiriand"), card("lorien", false)],
            forecast: vec![DiskForecast {
                host: "ossiriand".into(),
                mount: "/data".into(),
                size: 1.0,
                used: 0.9,
                per_day: Some(1024.0 * 1024.0 * 1024.0),
                days_left: Some(12.0),
            }],
            alerts: vec![AlertEvent {
                host: "ossiriand".into(),
                key: "disk".into(),
                kind: "disk".into(),
                detail: "disk 91% full".into(),
                severe: true,
                opened: 100.0,
                closed: Some(4000.0),
            }],
            detail: None,
        }),
        5000.0,
    );
    app
}

fn plain(app: &mut App, width: usize, height: usize) -> String {
    render(
        app,
        5003.0,
        0,
        width,
        height,
        &Theme::dark(),
        ColorLevel::None,
    )
    .join("\n")
}

#[quench_test::test]
fn the_host_list_shows_every_host_with_its_state() {
    let mut app = app();
    let screen = plain(&mut app, 119, 30);
    assert!(screen.contains("Gauge"), "{screen}");
    assert!(screen.contains("ossiriand"));
    assert!(screen.contains("lorien"));
    assert!(screen.contains("1 alert"), "{screen}");
    assert!(screen.contains("unreachable"), "{screen}");
    assert!(screen.contains("connection refused"));
    assert!(screen.contains("2 hosts · 1 up"), "{screen}");
    assert!(screen.contains("Updated just now"), "{screen}");
}

#[quench_test::test]
fn a_narrow_terminal_stacks_the_cards_in_one_column() {
    let mut app = app();
    let screen = plain(&mut app, 50, 60);
    assert!(screen.contains("ossiriand"), "{screen}");
    assert!(screen.contains("1 alert"), "{screen}");
}

#[quench_test::test]
fn no_row_is_wider_than_the_terminal() {
    for width in [30, 50, 80, 119, 200] {
        let mut app = app();
        for screen in Screen::ALL {
            app.screen = screen;
            let rows = render(
                &mut app,
                5003.0,
                0,
                width,
                24,
                &Theme::dark(),
                ColorLevel::None,
            );
            for row in rows {
                assert!(row.chars().count() <= width, "{width}: {row:?}");
            }
        }
        app.screen = Screen::Hosts;
        handle_key(&mut app, KeyEvent::from(KeyCode::Enter));
        let rows = render(
            &mut app,
            5003.0,
            0,
            width,
            24,
            &Theme::dark(),
            ColorLevel::None,
        );
        assert!(rows.iter().all(|r| r.chars().count() <= width));
    }
}

#[quench_test::test]
fn the_host_page_shows_its_sections() {
    let mut app = app();
    handle_key(&mut app, KeyEvent::from(KeyCode::Enter));
    // `lorien` sorts first and is down.
    let screen = plain(&mut app, 119, 60);
    assert!(screen.contains("connection refused"), "{screen}");
    handle_key(&mut app, KeyEvent::from(KeyCode::Esc));
    handle_key(&mut app, KeyEvent::from(KeyCode::Down));
    handle_key(&mut app, KeyEvent::from(KeyCode::Enter));
    let screen = plain(&mut app, 119, 80);
    for want in [
        "Overview",
        "CPU",
        "Memory",
        "GPU · gfx1201",
        "Filesystems",
        "/data",
        "CPU 93% for 5 minutes",
    ] {
        assert!(screen.contains(want), "missing {want}: {screen}");
    }
}

#[quench_test::test]
fn a_long_page_scrolls_and_the_scroll_is_clamped() {
    let mut app = app();
    handle_key(&mut app, KeyEvent::from(KeyCode::Down));
    handle_key(&mut app, KeyEvent::from(KeyCode::Enter));
    let top = plain(&mut app, 119, 18);
    assert!(top.contains("Overview"));
    assert!(top.contains(" of "), "scroll position is shown: {top}");
    handle_key(&mut app, KeyEvent::from(KeyCode::End));
    let bottom = plain(&mut app, 119, 18);
    assert!(!bottom.contains("Overview"), "{bottom}");
    assert!(app.scroll < usize::MAX, "clamped");
    assert!(app.scroll > 0);
}

#[quench_test::test]
fn the_alerts_page_lists_current_and_ended_alerts() {
    let mut app = app();
    app.screen = Screen::Alerts;
    let screen = plain(&mut app, 119, 40);
    assert!(screen.contains("lorien"), "{screen}");
    assert!(screen.contains("CPU 93% for 5 minutes"));
    assert!(screen.contains("disk 91% full"));
    assert!(screen.contains("lasted"), "{screen}");
}

#[quench_test::test]
fn severe_alerts_and_down_hosts_come_first() {
    let app = app();
    let now = current_alerts(&app.data);
    assert_eq!(now.len(), 2);
    assert_eq!(now[0].0, "lorien");
    assert!(now[0].1.severe);
}

#[quench_test::test]
fn the_storage_page_joins_the_forecast_to_the_filesystem() {
    let mut app = app();
    app.screen = Screen::Storage;
    let screen = plain(&mut app, 119, 40);
    assert!(screen.contains("/data"), "{screen}");
    assert!(screen.contains("full in 12d"), "{screen}");
}

#[quench_test::test]
fn the_status_line_reports_a_failure_and_the_first_wait() {
    let mut fresh = App::new();
    assert!(plain(&mut fresh, 100, 24).contains("Connecting"));
    assert!(plain(&mut fresh, 100, 24).contains("Waiting for the first answer"));
    let mut app = app();
    app.apply(Err("cannot reach the server".into()), 5001.0);
    assert!(plain(&mut app, 100, 24).contains("cannot reach the server"));
}

#[quench_test::test]
fn a_frame_homes_the_cursor_and_erases_below() {
    let out = frame(&["a".to_string(), "b".to_string()], "");
    assert!(out.starts_with("\x1b[H"));
    assert!(out.ends_with("\x1b[J\x1b[0m"));
}

#[quench_test::test]
fn the_host_page_lists_the_active_alerts_under_the_hosts() {
    let mut app = app();
    let screen = plain(&mut app, 119, 50);
    assert!(screen.contains("Active alerts"), "{screen}");
    assert!(screen.contains("CPU 93% for 5 minutes"), "{screen}");
    assert!(screen.contains("connection refused"));
    let mut quiet = App::new();
    quiet.apply(
        Ok(Data {
            hosts: vec![card("a", true)],
            ..Data::default()
        }),
        1.0,
    );
    assert!(plain(&mut quiet, 119, 50).contains("All clear"));
}

#[quench_test::test]
fn many_alerts_are_cut_with_a_pointer_to_the_alerts_page() {
    let mut app = App::new();
    let hosts = (0..8).map(|n| {
        let mut h = busy_host(&format!("host{n}"));
        h.host.up = true;
        h
    });
    app.apply(
        Ok(Data {
            hosts: hosts.collect(),
            ..Data::default()
        }),
        1.0,
    );
    let screen = plain(&mut app, 119, 40);
    assert!(screen.contains("and 3 more"), "{screen}");
}

#[quench_test::test]
fn the_fleet_page_has_tiles_and_adds_trend_pressure_hottest_and_alerts_columns() {
    let mut app = app();
    app.screen = Screen::Fleet;
    let screen = plain(&mut app, 119, 50);
    for want in [
        "Fleet",
        "CPU 1h",
        "Pressure",
        "Hottest",
        "Alerts",
        "CPU (average)",
        "1/2 up",
    ] {
        assert!(screen.contains(want), "missing {want}: {screen}");
    }
    let short = plain(&mut app, 119, 20);
    assert!(
        !short.contains("CPU (average)") && short.contains("ossiriand"),
        "{short}"
    );
}

#[quench_test::test]
fn the_gpu_network_and_power_pages_show_their_numbers() {
    let mut app = app();
    app.screen = Screen::Gpus;
    let gpus = plain(&mut app, 119, 40);
    assert!(gpus.contains("ossiriand · gfx1201"), "{gpus}");
    app.screen = Screen::Network;
    let net = plain(&mut app, 119, 40);
    assert!(net.contains("Retransmits"), "{net}");
    app.screen = Screen::Power;
    let power = plain(&mut app, 119, 40);
    assert!(
        power.contains("not what the wall socket draws") || power.contains("No power readings"),
        "{power}"
    );
    assert!(power.contains("Fans"));
}

#[quench_test::test]
fn the_storage_page_opens_with_a_summary() {
    let mut app = app();
    app.screen = Screen::Storage;
    let screen = plain(&mut app, 119, 40);
    for want in [
        "Capacity",
        "Free",
        "Fills first",
        "ossiriand /data in 12d",
        "Fullest",
    ] {
        assert!(screen.contains(want), "missing {want}: {screen}");
    }
}

#[quench_test::test]
fn the_host_list_has_tiles_on_a_tall_screen_and_not_on_a_short_one() {
    let mut app = app();
    let screen = plain(&mut app, 119, 50);
    assert!(screen.contains("CPU (average)"), "{screen}");
    assert!(screen.contains("1/2 up"), "{screen}");
    let short = plain(&mut app, 119, 20);
    assert!(
        !short.contains("CPU (average)"),
        "tiles give way on a short screen: {short}"
    );
    assert!(short.contains("ossiriand"), "{short}");
}

#[quench_test::test]
fn every_host_is_a_rounded_card_with_its_meters_and_footer() {
    let mut app = app();
    app.data.hosts[1].cpu = (0..30).map(|n| Some(f64::from(n) * 3.0)).collect();
    let screen = plain(&mut app, 119, 50);
    assert!(screen.contains("╭"), "{screen}");
    for want in [
        "ossiriand",
        "● up",
        "● unreachable",
        "1 alert",
        "node_exporter · http://ossiriand:9100",
        "CPU",
        "Memory",
        "GPU",
        "Disk",
        "Load 1.50",
        "↓",
        "connection refused",
    ] {
        assert!(screen.contains(want), "missing {want}: {screen}");
    }
    assert!(
        screen.contains('█') && screen.contains('▁'),
        "meters and the CPU chart: {screen}"
    );
    // Two cards on a row at this width.
    let row = screen.lines().find(|l| l.contains("lorien")).unwrap();
    assert!(row.contains("ossiriand"), "{row}");
    // One card per row on a narrow terminal.
    let narrow = plain(&mut app, 60, 60);
    let row = narrow.lines().find(|l| l.contains("lorien")).unwrap();
    assert!(!row.contains("ossiriand"), "{row}");
}

#[quench_test::test]
fn the_grid_follows_the_terminal_width_for_its_arrow_keys() {
    let mut app = app();
    plain(&mut app, 119, 50);
    assert_eq!(app.grid_cols, 2);
    plain(&mut app, 60, 60);
    assert_eq!(app.grid_cols, 1);
}

#[quench_test::test]
fn a_short_screen_shrinks_the_cards_instead_of_cutting_them() {
    let mut app = app();
    let screen = plain(&mut app, 119, 20);
    assert!(
        screen.contains("ossiriand") && screen.contains("Disk"),
        "{screen}"
    );
    assert!(
        !screen.contains("node_exporter ·"),
        "compact cards leave out the line under the name: {screen}"
    );
    assert!(screen.lines().count() <= 20, "it fits the screen");
}

#[quench_test::test]
fn the_host_page_is_a_set_of_rounded_cards() {
    let mut app = app();
    handle_key(&mut app, KeyEvent::from(KeyCode::Down));
    handle_key(&mut app, KeyEvent::from(KeyCode::Enter));
    let wide = plain(&mut app, 124, 60);
    let top = wide.lines().find(|l| l.contains("Overview")).unwrap();
    assert!(
        top.matches('╭').count() + top.matches('│').count() >= 3,
        "two cards side by side: {top}"
    );
    assert!(wide.contains("Alerts") && wide.contains("Filesystems") && wide.contains("Memory"));
    let narrow = plain(&mut app, 80, 80);
    let overview = narrow.lines().find(|l| l.contains("Overview")).unwrap();
    assert!(
        !overview.contains("Alerts"),
        "one column when narrow: {overview}"
    );
}

#[quench_test::test]
fn the_host_page_opens_with_tiles() {
    let mut app = app();
    handle_key(&mut app, KeyEvent::from(KeyCode::Down));
    handle_key(&mut app, KeyEvent::from(KeyCode::Enter));
    let screen = plain(&mut app, 119, 50);
    for want in ["CPU", "Memory", "GPU", "Disk", "Network"] {
        assert!(screen.contains(want), "missing {want}: {screen}");
    }
    assert!(screen.contains("no history"), "no detail yet: {screen}");
}
