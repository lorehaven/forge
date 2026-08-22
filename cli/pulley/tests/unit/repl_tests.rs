use pulley::config::{Config, Job};
use pulley::repl::{Repl, select_jobs};
use quench_cli::prelude::ReplControl;

fn job(id: &str) -> Job {
    Job {
        id: id.to_string(),
        desc: "desc".to_string(),
        src: "/src".to_string(),
        dest: "/dest".to_string(),
        delete: false,
        skip: Vec::new(),
        no_confirm: false,
        interval: None,
    }
}

fn job_with_dirs(id: &str, src: &std::path::Path, dest: &std::path::Path) -> Job {
    Job {
        id: id.to_string(),
        desc: "desc".to_string(),
        src: src.display().to_string(),
        dest: dest.display().to_string(),
        delete: false,
        skip: Vec::new(),
        no_confirm: true,
        interval: None,
    }
}

fn repl_with(jobs: Vec<Job>) -> Repl {
    Repl::new(Config { jobs })
}

fn assert_continues(control: ReplControl) {
    assert!(matches!(control, ReplControl::Continue(_)));
}

#[test]
fn select_jobs_all_returns_every_job() {
    let jobs = vec![job("a"), job("b"), job("c")];
    let selected = select_jobs(&jobs, &["all"]);
    assert_eq!(selected.len(), 3);
}

#[test]
fn select_jobs_by_id_keeps_the_configured_order_not_the_argument_order() {
    let jobs = vec![job("a"), job("b"), job("c")];
    let selected = select_jobs(&jobs, &["c", "a"]);
    let ids: Vec<&str> = selected.iter().map(|j| j.id.as_str()).collect();
    assert_eq!(ids, vec!["a", "c"]);
}

#[test]
fn select_jobs_ignores_unknown_ids() {
    let jobs = vec![job("a"), job("b")];
    let selected = select_jobs(&jobs, &["a", "does-not-exist"]);
    let ids: Vec<&str> = selected.iter().map(|j| j.id.as_str()).collect();
    assert_eq!(ids, vec!["a"]);
}

#[test]
fn select_jobs_with_no_matches_is_empty() {
    let jobs = vec![job("a"), job("b")];
    let selected = select_jobs(&jobs, &["nope"]);
    assert!(selected.is_empty());
}

#[test]
fn select_jobs_does_not_treat_all_as_a_literal_job_id() {
    // A job actually named "all" is shadowed by the `run all` shorthand -
    // documented behavior, exercised here so a future change to the
    // precedence is a deliberate one.
    let jobs = vec![job("all"), job("b")];
    let selected = select_jobs(&jobs, &["all"]);
    assert_eq!(selected.len(), 2);
}

#[test]
fn handle_command_quit_and_exit_end_the_session() {
    let mut repl = repl_with(vec![]);
    assert!(matches!(repl.handle_command("quit"), ReplControl::Exit));
    assert!(matches!(repl.handle_command("exit"), ReplControl::Exit));
}

#[test]
fn handle_command_empty_line_just_reprompts() {
    let mut repl = repl_with(vec![]);
    assert_continues(repl.handle_command(""));
    assert_continues(repl.handle_command("   "));
}

#[test]
fn handle_command_help_and_list_do_not_panic() {
    let mut repl = repl_with(vec![job("a")]);
    assert_continues(repl.handle_command("help"));
    assert_continues(repl.handle_command("list"));
}

#[test]
fn handle_command_list_on_an_empty_config_does_not_panic() {
    let mut repl = repl_with(vec![]);
    assert_continues(repl.handle_command("list"));
}

#[test]
fn handle_command_unknown_command_warns_and_continues() {
    let mut repl = repl_with(vec![]);
    assert_continues(repl.handle_command("frobnicate"));
}

#[test]
fn handle_command_run_with_no_args_prints_usage_and_continues() {
    let mut repl = repl_with(vec![job("a")]);
    assert_continues(repl.handle_command("run"));
}

#[test]
fn handle_command_run_with_an_unknown_job_id_finds_nothing() {
    let mut repl = repl_with(vec![job("a")]);
    assert_continues(repl.handle_command("run does-not-exist"));
}

#[test]
fn handle_command_reload_does_not_panic_regardless_of_real_config_state() {
    // `Config::load_merged()` is read-only (scans the real
    // `~/.config/pulley` and cwd for `*.pulley.toml`), so this is safe
    // to run for real even though it depends on ambient state this test
    // doesn't control - a parse error there is reported and handled,
    // not propagated as a panic.
    let mut repl = repl_with(vec![]);
    assert_continues(repl.handle_command("reload"));
}

#[test]
fn run_jobs_with_no_confirm_syncs_without_prompting() {
    let src = tempfile::tempdir().unwrap();
    let dest = tempfile::tempdir().unwrap();
    std::fs::write(src.path().join("a.txt"), "hello").unwrap();

    let job = job_with_dirs("sync-me", src.path(), dest.path());
    let repl = repl_with(vec![job.clone()]);

    repl.run_jobs(&["sync-me"]).expect("run_jobs");
    assert_eq!(
        std::fs::read_to_string(dest.path().join("a.txt")).unwrap(),
        "hello"
    );
}

#[test]
fn run_jobs_with_nothing_to_sync_reports_no_changes_without_erroring() {
    let src = tempfile::tempdir().unwrap();
    let dest = tempfile::tempdir().unwrap();
    let job = job_with_dirs("nothing-to-do", src.path(), dest.path());
    let repl = repl_with(vec![job]);

    repl.run_jobs(&["nothing-to-do"]).expect("run_jobs");
}

#[test]
fn run_jobs_with_empty_args_prints_usage_without_erroring() {
    let repl = repl_with(vec![job("a")]);
    repl.run_jobs(&[]).expect("run_jobs");
}
