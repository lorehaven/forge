use crossterm::event::KeyCode;
use forge_toolbox::{
    ActionDispatch, ActionRequest, App, CrateStatus, InFlightAction, KeyEffect, KeyOutcome,
    PollOutcome, TickEffect, char_width, dispatch_action, handle_key, on_key, on_tick,
    poll_in_flight, transient_status,
};
use std::sync::mpsc;
use std::time::{Duration, Instant};

fn status(package: &'static str) -> CrateStatus {
    CrateStatus {
        package,
        binary: package,
        installed_version: None,
        latest_version: None,
        installable: true,
        updatable: false,
        error: None,
    }
}

#[test]
fn up_always_moves() {
    let statuses = [status("a"), status("b")];
    assert_eq!(
        handle_key(KeyCode::Up, 1, statuses.len(), false, &statuses),
        KeyOutcome::MoveUp
    );
}

#[test]
fn down_is_ignored_at_the_end_of_the_list() {
    let statuses = [status("a"), status("b")];
    assert_eq!(
        handle_key(KeyCode::Down, 1, statuses.len(), false, &statuses),
        KeyOutcome::Ignore
    );
}

#[test]
fn down_moves_when_not_at_the_end() {
    let statuses = [status("a"), status("b")];
    assert_eq!(
        handle_key(KeyCode::Down, 0, statuses.len(), false, &statuses),
        KeyOutcome::MoveDown
    );
}

#[test]
fn q_quits() {
    let statuses = [status("a")];
    assert_eq!(
        handle_key(KeyCode::Char('q'), 0, statuses.len(), false, &statuses),
        KeyOutcome::Quit
    );
}

#[test]
fn r_refreshes_when_idle() {
    let statuses = [status("a")];
    assert_eq!(
        handle_key(KeyCode::Char('r'), 0, statuses.len(), false, &statuses),
        KeyOutcome::Refresh
    );
}

#[test]
fn r_is_busy_while_an_action_is_running() {
    let statuses = [status("a")];
    assert_eq!(
        handle_key(KeyCode::Char('r'), 0, statuses.len(), true, &statuses),
        KeyOutcome::Busy
    );
}

#[test]
fn enter_is_busy_while_an_action_is_running() {
    let statuses = [status("a")];
    assert_eq!(
        handle_key(KeyCode::Enter, 0, statuses.len(), true, &statuses),
        KeyOutcome::Busy
    );
}

#[test]
fn enter_runs_the_action_for_the_selected_row() {
    let statuses = [status("a"), status("b")];
    match handle_key(KeyCode::Enter, 1, statuses.len(), false, &statuses) {
        KeyOutcome::RunAction(req) => assert_eq!(req.package, "b"),
        other => panic!("expected RunAction, got {other:?}"),
    }
}

#[test]
fn enter_with_an_out_of_range_selection_is_ignored() {
    let statuses = [status("a")];
    assert_eq!(
        handle_key(KeyCode::Enter, 5, statuses.len(), false, &statuses),
        KeyOutcome::Ignore
    );
}

#[test]
fn other_keys_are_ignored() {
    let statuses = [status("a")];
    assert_eq!(
        handle_key(KeyCode::Char('x'), 0, statuses.len(), false, &statuses),
        KeyOutcome::Ignore
    );
}

fn job(rx: mpsc::Receiver<Result<String, String>>) -> InFlightAction {
    InFlightAction {
        rx,
        selected: 2,
        spinner_idx: 0,
        label: "running a".to_string(),
        last_tick: Instant::now(),
    }
}

#[test]
fn poll_resolves_ok_results() {
    let (tx, rx) = mpsc::channel();
    tx.send(Ok("a installed".to_string())).unwrap();
    let mut job = job(rx);

    match poll_in_flight(&mut job, Duration::from_millis(120)) {
        PollOutcome::Done { selected, message } => {
            assert_eq!(selected, 2);
            assert_eq!(message, "a installed");
        }
        other => panic!("expected Done, got {other:?}"),
    }
}

#[test]
fn poll_folds_err_results_into_a_failure_message() {
    let (tx, rx) = mpsc::channel();
    tx.send(Err("boom".to_string())).unwrap();
    let mut job = job(rx);

    match poll_in_flight(&mut job, Duration::from_millis(120)) {
        PollOutcome::Done { message, .. } => assert_eq!(message, "action failed: boom"),
        other => panic!("expected Done, got {other:?}"),
    }
}

#[test]
fn poll_reports_pending_before_the_tick_interval_elapses() {
    let (_tx, rx) = mpsc::channel();
    let mut job = job(rx);

    assert_eq!(
        poll_in_flight(&mut job, Duration::from_secs(3600)),
        PollOutcome::Pending
    );
}

#[test]
fn poll_advances_the_spinner_once_the_tick_interval_elapses() {
    let (_tx, rx) = mpsc::channel();
    let mut job = job(rx);
    job.last_tick = Instant::now() - Duration::from_millis(200);

    assert_eq!(
        poll_in_flight(&mut job, Duration::from_millis(120)),
        PollOutcome::Tick
    );
    assert_eq!(job.spinner_idx, 1);
}

#[test]
fn poll_resolves_with_a_failure_message_once_the_sender_is_dropped() {
    let (tx, rx) = mpsc::channel::<Result<String, String>>();
    drop(tx);
    let mut job = job(rx);

    match poll_in_flight(&mut job, Duration::from_millis(120)) {
        PollOutcome::Done { message, .. } => {
            assert_eq!(message, "action failed: worker disconnected");
        }
        other => panic!("expected Done, got {other:?}"),
    }
}

#[test]
fn dispatch_action_resolves_not_installable_synchronously() {
    let req = ActionRequest {
        package: "mystery".to_string(),
        installable: false,
        installed: false,
        updatable: false,
    };
    match dispatch_action(req) {
        ActionDispatch::Done(message) => assert!(message.contains("not installable")),
        other => panic!("expected Done, got {other:?}"),
    }
}

#[test]
fn dispatch_action_resolves_already_up_to_date_synchronously() {
    let req = ActionRequest {
        package: "anvil".to_string(),
        installable: true,
        installed: true,
        updatable: false,
    };
    match dispatch_action(req) {
        ActionDispatch::Done(message) => assert_eq!(message, "anvil is already up to date"),
        other => panic!("expected Done, got {other:?}"),
    }
}

#[test]
fn dispatch_action_spawns_for_a_fresh_install() {
    let req = ActionRequest {
        package: "anvil".to_string(),
        installable: true,
        installed: false,
        updatable: false,
    };
    match dispatch_action(req) {
        ActionDispatch::Spawn { label } => assert_eq!(label, "running anvil (installing)"),
        other => panic!("expected Spawn, got {other:?}"),
    }
}

#[test]
fn dispatch_action_spawns_for_an_update() {
    let req = ActionRequest {
        package: "anvil".to_string(),
        installable: true,
        installed: true,
        updatable: true,
    };
    match dispatch_action(req) {
        ActionDispatch::Spawn { label } => assert_eq!(label, "running anvil (updating)"),
        other => panic!("expected Spawn, got {other:?}"),
    }
}

#[test]
fn action_request_round_trips_through_run_action() {
    let statuses = [status("a")];
    let req = ActionRequest {
        package: "a".to_string(),
        installable: true,
        installed: false,
        updatable: false,
    };
    match handle_key(KeyCode::Enter, 0, statuses.len(), false, &statuses) {
        KeyOutcome::RunAction(got) => assert_eq!(got, req),
        other => panic!("expected RunAction, got {other:?}"),
    }
}

#[test]
fn char_width_counts_unicode_scalars_not_bytes() {
    assert_eq!(char_width("abc"), 3);
    assert_eq!(char_width(""), 0);
    assert_eq!(char_width("héllo"), 5);
}

#[test]
fn transient_status_is_none_when_nothing_is_running() {
    assert_eq!(transient_status(&None), None);
}

#[test]
fn transient_status_shows_the_spinner_frame_and_label_when_running() {
    let (_tx, rx) = mpsc::channel();
    let running = Some(job(rx));
    let status = transient_status(&running).expect("should be Some while running");
    assert!(status.ends_with("running a"));
}

#[test]
fn on_tick_is_none_when_nothing_is_running() {
    let mut in_flight = None;
    assert_eq!(
        on_tick(&mut in_flight, Duration::from_millis(120)),
        TickEffect::None
    );
}

#[test]
fn on_tick_redraws_when_the_spinner_advances() {
    let (_tx, rx) = mpsc::channel();
    let mut running_job = job(rx);
    running_job.last_tick = Instant::now() - Duration::from_millis(200);
    let mut in_flight = Some(running_job);

    assert_eq!(
        on_tick(&mut in_flight, Duration::from_millis(120)),
        TickEffect::Redraw
    );
    assert!(in_flight.is_some());
}

#[test]
fn on_tick_clears_in_flight_and_reports_refresh_once_resolved() {
    let (tx, rx) = mpsc::channel();
    tx.send(Ok("a installed".to_string())).unwrap();
    let mut in_flight = Some(job(rx));

    match on_tick(&mut in_flight, Duration::from_millis(120)) {
        TickEffect::Refresh { selected, message } => {
            assert_eq!(selected, 2);
            assert_eq!(message, "a installed");
        }
        other => panic!("expected Refresh, got {other:?}"),
    }
    assert!(in_flight.is_none());
}

fn app(statuses: Vec<CrateStatus>) -> App {
    App {
        selected: 0,
        statuses,
        toolbox_note: String::new(),
        message: "Ready".to_string(),
    }
}

#[test]
fn on_key_moves_the_selection_and_reports_redraw() {
    let mut state = app(vec![status("a"), status("b")]);
    assert_eq!(on_key(&mut state, false, KeyCode::Down), KeyEffect::Redraw);
    assert_eq!(state.selected, 1);
}

#[test]
fn on_key_moves_up_and_reports_redraw() {
    let mut state = app(vec![status("a"), status("b")]);
    state.selected = 1;
    assert_eq!(on_key(&mut state, false, KeyCode::Up), KeyEffect::Redraw);
    assert_eq!(state.selected, 0);
}

#[test]
fn on_tick_is_none_while_pending_before_the_tick_interval_elapses() {
    let (_tx, rx) = mpsc::channel();
    let mut in_flight = Some(job(rx));
    assert_eq!(
        on_tick(&mut in_flight, Duration::from_secs(3600)),
        TickEffect::None
    );
    assert!(in_flight.is_some());
}

#[test]
fn on_key_quits_on_q() {
    let mut state = app(vec![status("a")]);
    assert_eq!(
        on_key(&mut state, false, KeyCode::Char('q')),
        KeyEffect::Quit
    );
}

#[test]
fn on_key_refreshes_on_r_when_idle() {
    let mut state = app(vec![status("a")]);
    match on_key(&mut state, false, KeyCode::Char('r')) {
        KeyEffect::Refresh { selected, message } => {
            assert_eq!(selected, 0);
            assert_eq!(message, "Status refreshed");
        }
        other => panic!("expected Refresh, got {other:?}"),
    }
}

#[test]
fn on_key_is_none_while_busy() {
    let mut state = app(vec![status("a")]);
    assert_eq!(
        on_key(&mut state, true, KeyCode::Char('r')),
        KeyEffect::None
    );
}

#[test]
fn on_key_spawns_for_an_installable_action() {
    let mut state = app(vec![status("a")]);
    match on_key(&mut state, false, KeyCode::Enter) {
        KeyEffect::Spawn {
            selected,
            label,
            req,
        } => {
            assert_eq!(selected, 0);
            assert_eq!(label, "running a (installing)");
            assert_eq!(req.package, "a");
        }
        other => panic!("expected Spawn, got {other:?}"),
    }
}

#[test]
fn on_key_resolves_synchronously_for_a_not_installable_action() {
    let mut state = app(vec![CrateStatus {
        package: "b",
        binary: "b",
        installed_version: None,
        latest_version: None,
        installable: false,
        updatable: false,
        error: None,
    }]);
    match on_key(&mut state, false, KeyCode::Enter) {
        KeyEffect::Refresh { message, .. } => assert!(message.contains("not installable")),
        other => panic!("expected Refresh, got {other:?}"),
    }
}
