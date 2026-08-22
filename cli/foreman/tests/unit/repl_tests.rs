use foreman::config::Config;
use foreman::estate::Estate;
use foreman::repl::Picker;
use foreman::vars;
use quench_cli::prelude::ReplControl;
use std::path::Path;

const TWO_SERVICES: &str = r#"
        [[services]]
        name = "web"
        package = "web-svc"
        port = 8080

        [[services]]
        name = "worker"
        package = "worker-svc"
        port = 8081
    "#;

/// Caller keeps `root`'s `TempDir` alive for as long as the returned
/// `Estate` is in use.
fn estate(root: &Path, text: &str) -> Estate {
    let config: Config = toml::from_str(text).unwrap();
    let vars = vars::resolve(root, &config.vars).unwrap();
    Estate {
        root: root.to_path_buf(),
        config_path: root.join("foreman.toml"),
        config,
        vars,
    }
}

#[test]
fn new_starts_with_nothing_picked() {
    let dir = tempfile::tempdir().unwrap();
    let e = estate(dir.path(), TWO_SERVICES);
    let picker = Picker::new(&e);
    assert_eq!(picker.prompt(), "foreman> ");
}

#[test]
fn seed_resolves_names_into_the_initial_selection() {
    let dir = tempfile::tempdir().unwrap();
    let e = estate(dir.path(), TWO_SERVICES);
    let mut picker = Picker::new(&e);
    picker.seed(&["web".to_string()]).unwrap();
    assert_eq!(picker.prompt(), "foreman (web)> ");
}

#[test]
fn seed_with_no_names_leaves_the_selection_empty() {
    let dir = tempfile::tempdir().unwrap();
    let e = estate(dir.path(), TWO_SERVICES);
    let mut picker = Picker::new(&e);
    picker.seed(&[]).unwrap();
    assert_eq!(picker.prompt(), "foreman> ");
}

#[test]
fn seed_errors_on_an_unknown_name() {
    let dir = tempfile::tempdir().unwrap();
    let e = estate(dir.path(), TWO_SERVICES);
    let mut picker = Picker::new(&e);
    assert!(picker.seed(&["no-such-service".to_string()]).is_err());
}

#[test]
fn dispatch_line_on_quit_exit_or_q_exits() {
    let dir = tempfile::tempdir().unwrap();
    let e = estate(dir.path(), TWO_SERVICES);
    let mut picker = Picker::new(&e);
    for word in ["quit", "exit", "q"] {
        assert!(matches!(picker.dispatch_line(word), ReplControl::Exit));
    }
}

#[test]
fn dispatch_line_on_a_blank_line_continues_with_the_current_prompt() {
    let dir = tempfile::tempdir().unwrap();
    let e = estate(dir.path(), TWO_SERVICES);
    let mut picker = Picker::new(&e);
    match picker.dispatch_line("") {
        ReplControl::Continue(prompt) => assert_eq!(prompt, "foreman> "),
        ReplControl::Exit => panic!("expected Continue"),
    }
}

#[test]
fn dispatch_line_toggles_a_service_by_name_and_continues() {
    let dir = tempfile::tempdir().unwrap();
    let e = estate(dir.path(), TWO_SERVICES);
    let mut picker = Picker::new(&e);
    match picker.dispatch_line("web") {
        ReplControl::Continue(prompt) => assert_eq!(prompt, "foreman (web)> "),
        ReplControl::Exit => panic!("expected Continue"),
    }
}

#[test]
fn dispatch_line_on_an_out_of_range_index_still_continues() {
    let dir = tempfile::tempdir().unwrap();
    let e = estate(dir.path(), TWO_SERVICES);
    let mut picker = Picker::new(&e);
    let control = picker.dispatch_line("999");
    assert!(matches!(control, ReplControl::Continue(_)));
}

#[test]
fn toggle_turns_a_service_on_then_off_by_name() {
    let dir = tempfile::tempdir().unwrap();
    let e = estate(dir.path(), TWO_SERVICES);
    let mut picker = Picker::new(&e);
    picker.toggle("web").unwrap();
    assert_eq!(picker.prompt(), "foreman (web)> ");
    picker.toggle("web").unwrap();
    assert_eq!(picker.prompt(), "foreman> ");
}

#[test]
fn toggle_accepts_a_one_based_index() {
    let dir = tempfile::tempdir().unwrap();
    let e = estate(dir.path(), TWO_SERVICES);
    let mut picker = Picker::new(&e);
    picker.toggle("2").unwrap();
    assert_eq!(picker.prompt(), "foreman (worker)> ");
}

#[test]
fn toggle_warns_but_does_not_error_on_an_out_of_range_index() {
    let dir = tempfile::tempdir().unwrap();
    let e = estate(dir.path(), TWO_SERVICES);
    let mut picker = Picker::new(&e);
    picker.toggle("99").unwrap();
    assert_eq!(picker.prompt(), "foreman> ");
}

#[test]
fn toggle_warns_but_does_not_error_on_an_unknown_name() {
    let dir = tempfile::tempdir().unwrap();
    let e = estate(dir.path(), TWO_SERVICES);
    let mut picker = Picker::new(&e);
    picker.toggle("no-such-service").unwrap();
    assert_eq!(picker.prompt(), "foreman> ");
}

#[test]
fn toggle_keeps_the_selection_in_table_order_regardless_of_toggle_order() {
    let dir = tempfile::tempdir().unwrap();
    let e = estate(dir.path(), TWO_SERVICES);
    let mut picker = Picker::new(&e);
    picker.toggle("worker").unwrap();
    picker.toggle("web").unwrap();
    assert_eq!(picker.prompt(), "foreman (web,worker)> ");
}

#[test]
fn dispatch_all_selects_every_service() {
    let dir = tempfile::tempdir().unwrap();
    let e = estate(dir.path(), TWO_SERVICES);
    let mut picker = Picker::new(&e);
    picker.dispatch("all", &[]).unwrap();
    assert_eq!(picker.prompt(), "foreman (web,worker)> ");
}

#[test]
fn dispatch_none_clears_the_selection() {
    let dir = tempfile::tempdir().unwrap();
    let e = estate(dir.path(), TWO_SERVICES);
    let mut picker = Picker::new(&e);
    picker.dispatch("all", &[]).unwrap();
    picker.dispatch("none", &[]).unwrap();
    assert_eq!(picker.prompt(), "foreman> ");
}

#[test]
fn dispatch_running_selects_nothing_when_nothing_is_up() {
    let dir = tempfile::tempdir().unwrap();
    let e = estate(dir.path(), TWO_SERVICES);
    let mut picker = Picker::new(&e);
    picker.dispatch("all", &[]).unwrap();
    picker.dispatch("running", &[]).unwrap();
    assert_eq!(picker.prompt(), "foreman> ");
}

#[test]
fn dispatch_status_succeeds_with_nothing_running_and_no_containers() {
    let dir = tempfile::tempdir().unwrap();
    let e = estate(dir.path(), TWO_SERVICES);
    let mut picker = Picker::new(&e);
    picker.dispatch("status", &[]).unwrap();
}

#[test]
fn dispatch_help_list_and_l_succeed() {
    let dir = tempfile::tempdir().unwrap();
    let e = estate(dir.path(), TWO_SERVICES);
    let mut picker = Picker::new(&e);
    for command in ["help", "h", "?", "list", "ls", "l"] {
        picker.dispatch(command, &[]).unwrap();
    }
}

#[test]
fn dispatch_up_with_nothing_selected_warns_rather_than_starting_anything() {
    let dir = tempfile::tempdir().unwrap();
    let e = estate(dir.path(), TWO_SERVICES);
    let mut picker = Picker::new(&e);
    // Nothing picked, so this hits the "nothing selected" warning branch
    // and returns Ok without ever calling `commands::start` for real.
    picker.dispatch("up", &[]).unwrap();
}

#[test]
fn dispatch_down_with_nothing_selected_warns_rather_than_stopping_anything() {
    let dir = tempfile::tempdir().unwrap();
    let e = estate(dir.path(), TWO_SERVICES);
    let mut picker = Picker::new(&e);
    picker.dispatch("down", &[]).unwrap();
}

#[test]
fn dispatch_logs_with_no_argument_warns_rather_than_following_anything() {
    let dir = tempfile::tempdir().unwrap();
    let e = estate(dir.path(), TWO_SERVICES);
    let mut picker = Picker::new(&e);
    picker.dispatch("logs", &[]).unwrap();
}

#[test]
fn dispatch_run_with_no_task_argument_warns_rather_than_running_anything() {
    let dir = tempfile::tempdir().unwrap();
    let e = estate(dir.path(), TWO_SERVICES);
    let mut picker = Picker::new(&e);
    picker.dispatch("run", &[]).unwrap();
}

#[test]
fn help_prints_without_panicking() {
    foreman::repl::help();
}
