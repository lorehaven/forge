use gauge_cli::state::{App, Data, Effect, Screen, handle_key};
use gauge_core::{HostCard, HostSummary};
use quench_cli::keys::{KeyCode, KeyEvent, Modifiers};

pub fn card(name: &str, up: bool) -> HostCard {
    HostCard {
        host: HostSummary {
            name: name.to_string(),
            kind: "node_exporter".to_string(),
            url: format!("http://{name}:9100"),
            up,
            error: (!up).then(|| "connection refused".to_string()),
            last_ok: None,
            view: None,
        },
        cpu: Vec::new(),
    }
}

fn app_with(names: &[&str]) -> App {
    let mut app = App::new();
    app.apply(
        Ok(Data {
            hosts: names.iter().map(|n| card(n, true)).collect(),
            ..Data::default()
        }),
        100.0,
    );
    app
}

fn press(app: &mut App, code: KeyCode) -> Effect {
    handle_key(app, KeyEvent::from(code))
}

#[quench_test::test]
fn hosts_are_sorted_by_name() {
    let app = app_with(&["b", "a", "c"]);
    let names: Vec<_> = app
        .data
        .hosts
        .iter()
        .map(|h| h.host.name.as_str())
        .collect();
    assert_eq!(names, ["a", "b", "c"]);
}

#[quench_test::test]
fn arrows_and_vim_keys_move_the_selection_within_the_list() {
    let mut app = app_with(&["a", "b", "c"]);
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Char('j'));
    press(&mut app, KeyCode::Down);
    assert_eq!(app.selected, 2, "stops at the last host");
    press(&mut app, KeyCode::Up);
    press(&mut app, KeyCode::Char('k'));
    press(&mut app, KeyCode::Up);
    assert_eq!(app.selected, 0, "stops at the first");
    press(&mut app, KeyCode::End);
    assert_eq!(app.selected, 2);
    press(&mut app, KeyCode::Home);
    assert_eq!(app.selected, 0);
}

#[quench_test::test]
fn enter_opens_the_selected_host_and_escape_closes_it() {
    let mut app = app_with(&["a", "b"]);
    press(&mut app, KeyCode::Down);
    assert_eq!(
        press(&mut app, KeyCode::Enter),
        Effect::Refresh,
        "opening a host fetches its history now"
    );
    assert_eq!(app.open_host().map(|h| h.host.name.as_str()), Some("b"));
    press(&mut app, KeyCode::Down);
    assert_eq!(app.scroll, 1, "arrows scroll an open page");
    assert_eq!(app.selected, 1, "and leave the list alone");
    press(&mut app, KeyCode::Esc);
    assert!(app.detail.is_none());
    assert_eq!(app.scroll, 0);
}

#[quench_test::test]
fn enter_on_an_empty_list_does_nothing() {
    let mut app = App::new();
    assert_eq!(press(&mut app, KeyCode::Enter), Effect::Nothing);
    assert!(app.detail.is_none());
}

#[quench_test::test]
fn tab_and_numbers_switch_pages_and_close_a_host() {
    let mut app = app_with(&["a"]);
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Tab);
    assert_eq!(app.screen, Screen::Fleet);
    assert!(app.detail.is_none());
    for want in [
        Screen::Storage,
        Screen::Gpus,
        Screen::Network,
        Screen::Power,
        Screen::Alerts,
    ] {
        press(&mut app, KeyCode::Tab);
        assert_eq!(app.screen, want);
    }
    press(&mut app, KeyCode::Tab);
    assert_eq!(app.screen, Screen::Hosts, "wraps");
    press(&mut app, KeyCode::BackTab);
    assert_eq!(app.screen, Screen::Alerts);
    for (key, want) in Screen::ALL.iter().enumerate() {
        press(
            &mut app,
            KeyCode::Char(char::from_digit(u32::try_from(key).unwrap() + 1, 10).unwrap()),
        );
        assert_eq!(app.screen, *want);
    }
    press(&mut app, KeyCode::Char('8'));
    assert_eq!(app.screen, Screen::Alerts, "there is no eighth page");
}

#[quench_test::test]
fn the_fleet_page_selects_and_opens_hosts_like_the_host_list() {
    let mut app = app_with(&["a", "b"]);
    press(&mut app, KeyCode::Char('2'));
    press(&mut app, KeyCode::Down);
    assert_eq!(app.selected, 1);
    press(&mut app, KeyCode::Right);
    assert_eq!(app.open_host().map(|h| h.host.name.as_str()), Some("b"));
}

#[quench_test::test]
fn page_keys_move_by_a_page() {
    let mut app = app_with(&["a"]);
    app.screen = Screen::Alerts;
    app.page_rows = 7;
    press(&mut app, KeyCode::PageDown);
    assert_eq!(app.scroll, 7);
    press(&mut app, KeyCode::PageUp);
    assert_eq!(app.scroll, 0);
    press(&mut app, KeyCode::End);
    assert_eq!(app.scroll, usize::MAX, "clamped by the next draw");
}

#[quench_test::test]
fn quit_and_refresh_keys() {
    let mut app = App::new();
    assert_eq!(press(&mut app, KeyCode::Char('q')), Effect::Quit);
    assert_eq!(press(&mut app, KeyCode::Char('r')), Effect::Refresh);
    let ctrl_c = KeyEvent {
        code: KeyCode::Char('c'),
        modifiers: Modifiers {
            ctrl: true,
            alt: false,
            shift: false,
        },
    };
    assert_eq!(handle_key(&mut app, ctrl_c), Effect::Quit);
    assert_eq!(press(&mut app, KeyCode::Char('x')), Effect::Nothing);
}

#[quench_test::test]
fn a_failed_fetch_keeps_the_numbers_and_says_why() {
    let mut app = app_with(&["a", "b"]);
    app.loading = true;
    app.apply(Err("cannot reach the server".to_string()), 200.0);
    assert!(!app.loading);
    assert_eq!(app.data.hosts.len(), 2);
    assert_eq!(app.fetched_at, Some(100.0));
    assert_eq!(app.error.as_deref(), Some("cannot reach the server"));
    app.apply(Ok(Data::default()), 300.0);
    assert!(app.error.is_none());
}

#[quench_test::test]
fn a_shrinking_list_keeps_the_selection_in_range() {
    let mut app = app_with(&["a", "b", "c"]);
    press(&mut app, KeyCode::End);
    app.apply(
        Ok(Data {
            hosts: vec![card("a", true)],
            ..Data::default()
        }),
        1.0,
    );
    assert_eq!(app.selected, 0);
}

#[quench_test::test]
fn the_host_grid_moves_by_cell_and_by_row() {
    let mut app = app_with(&["a", "b", "c", "d", "e"]);
    app.grid_cols = 2;
    press(&mut app, KeyCode::Right);
    assert_eq!(app.selected, 1);
    press(&mut app, KeyCode::Down);
    assert_eq!(app.selected, 3, "down is a row");
    press(&mut app, KeyCode::Char('h'));
    assert_eq!(app.selected, 2);
    press(&mut app, KeyCode::Char('j'));
    assert_eq!(app.selected, 4);
    press(&mut app, KeyCode::Down);
    assert_eq!(app.selected, 4, "stops at the last card");
    press(&mut app, KeyCode::Up);
    press(&mut app, KeyCode::Up);
    press(&mut app, KeyCode::Up);
    assert_eq!(app.selected, 0);
    press(&mut app, KeyCode::Left);
    assert_eq!(app.selected, 0);
}

#[quench_test::test]
fn arrows_sideways_do_not_open_a_card_but_enter_does() {
    let mut app = app_with(&["a", "b"]);
    press(&mut app, KeyCode::Right);
    press(&mut app, KeyCode::Char('l'));
    assert!(app.detail.is_none());
    assert_eq!(press(&mut app, KeyCode::Enter), Effect::Refresh);
    assert_eq!(app.detail.as_deref(), Some("b"));
    press(&mut app, KeyCode::Left);
    assert!(app.detail.is_none(), "left goes back from an open host");
}
