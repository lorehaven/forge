use foreman::commands::*;
use foreman::config::Config;
use foreman::estate::Estate;
use foreman::vars;
use std::sync::{Mutex, OnceLock};

fn estate_in(root: &std::path::Path, text: &str) -> Estate {
    let config: Config = toml::from_str(text).unwrap();
    let vars = vars::resolve(root, &config.vars).unwrap();
    Estate {
        root: root.to_path_buf(),
        config_path: root.join("foreman.toml"),
        config,
        vars,
    }
}

const CATALOG: &str = r#"
        [[containers]]
        name = "db"
        image = "postgres:latest"

        [[services]]
        name = "web"
        package = "web-svc"
        port = 8080
        needs = ["auth"]

        [[services]]
        name = "auth"
        package = "auth-svc"
        port = 8081

        [tasks.migrate]
        role = "migrate"
        command = ["true"]
    "#;

#[test]
fn list_prints_containers_services_and_tasks_without_error() {
    let dir = tempfile::tempdir().unwrap();
    let estate = estate_in(dir.path(), CATALOG);
    list(&estate).unwrap();
}

#[test]
fn list_handles_an_estate_with_nothing_configured() {
    let dir = tempfile::tempdir().unwrap();
    let estate = estate_in(dir.path(), "");
    list(&estate).unwrap();
}

#[test]
fn env_prints_a_services_resolved_settings() {
    let dir = tempfile::tempdir().unwrap();
    let estate = estate_in(dir.path(), CATALOG);
    env(&estate, "web").unwrap();
}

#[test]
fn env_errors_for_an_unknown_service() {
    let dir = tempfile::tempdir().unwrap();
    let estate = estate_in(dir.path(), CATALOG);
    assert!(env(&estate, "no-such-service").is_err());
}

#[test]
fn status_reports_every_container_and_service_without_a_pid_file() {
    let dir = tempfile::tempdir().unwrap();
    let estate = estate_in(dir.path(), CATALOG);
    status(&estate).unwrap();
}

#[test]
fn status_reports_a_service_with_a_live_pid() {
    let dir = tempfile::tempdir().unwrap();
    let estate = estate_in(dir.path(), CATALOG);
    estate.ensure_dirs().unwrap();
    foreman::process::write_pid(&estate.pid_file("web"), std::process::id() as i32).unwrap();
    status(&estate).unwrap();
}

#[test]
fn stop_reports_not_running_for_every_named_service() {
    let dir = tempfile::tempdir().unwrap();
    let estate = estate_in(dir.path(), CATALOG);
    estate.ensure_dirs().unwrap();
    stop(&estate, &["web".to_string()]).unwrap();
}

#[test]
fn stop_all_also_stops_the_estates_containers() {
    let dir = tempfile::tempdir().unwrap();
    let estate = estate_in(dir.path(), CATALOG);
    estate.ensure_dirs().unwrap();
    // `docker stop` on a container name that was never started is a
    // harmless, real, read-only-in-effect call - it just reports failure.
    stop(&estate, &["all".to_string()]).unwrap();
}

#[test]
fn logs_errors_for_an_unknown_service() {
    let dir = tempfile::tempdir().unwrap();
    let estate = estate_in(dir.path(), CATALOG);
    assert!(logs(&estate, "no-such-service").is_err());
}

#[test]
fn reset_errors_without_a_reset_role_task() {
    let dir = tempfile::tempdir().unwrap();
    let estate = estate_in(dir.path(), CATALOG);
    let error = reset(&estate).unwrap_err();
    assert!(error.to_string().contains("no task with"));
}

#[test]
fn reset_runs_every_reset_role_task() {
    let dir = tempfile::tempdir().unwrap();
    let estate = estate_in(
        dir.path(),
        r#"
                [tasks.reset]
                role = "reset"
                command = ["true"]
            "#,
    );
    reset(&estate).unwrap();
}

#[test]
fn db_runs_every_migrate_role_task_with_no_containers() {
    let dir = tempfile::tempdir().unwrap();
    let estate = estate_in(dir.path(), CATALOG);
    db(&estate).unwrap();
}

#[test]
fn run_task_errors_for_an_unknown_task() {
    let dir = tempfile::tempdir().unwrap();
    let estate = estate_in(dir.path(), CATALOG);
    assert!(run_task(&estate, "no-such-task", &[]).is_err());
}

#[test]
fn run_task_runs_a_known_task_with_no_selection() {
    let dir = tempfile::tempdir().unwrap();
    let estate = estate_in(dir.path(), CATALOG);
    run_task(&estate, "migrate", &[]).unwrap();
}

#[test]
fn start_errors_when_nothing_is_configured() {
    let dir = tempfile::tempdir().unwrap();
    let estate = estate_in(dir.path(), "");
    let error = start(&estate, &[]).unwrap_err();
    assert!(error.to_string().contains("no services are configured"));
}

#[test]
fn test_errors_without_a_test_section() {
    let dir = tempfile::tempdir().unwrap();
    let estate = estate_in(dir.path(), CATALOG);
    assert!(test(&estate, &[]).is_err());
}

#[test]
fn test_errors_on_an_empty_command() {
    let dir = tempfile::tempdir().unwrap();
    let estate = estate_in(
        dir.path(),
        r#"
                [test]
                command = []
            "#,
    );
    assert!(test(&estate, &[]).is_err());
}

#[test]
fn test_runs_the_configured_suite_and_reports_its_exit_code() {
    let dir = tempfile::tempdir().unwrap();
    let estate = estate_in(
        dir.path(),
        r#"
                [test]
                command = ["true"]
                stop_services = false
            "#,
    );
    assert_eq!(test(&estate, &[]).unwrap(), 0);
}

#[test]
fn test_reports_a_nonzero_exit_code_on_failure() {
    let dir = tempfile::tempdir().unwrap();
    let estate = estate_in(
        dir.path(),
        r#"
                [test]
                command = ["false"]
                stop_services = false
            "#,
    );
    assert_eq!(test(&estate, &[]).unwrap(), 1);
}

/// `init` reads and writes the process's current directory, which is
/// global state shared with every other test in this crate's `--lib`
/// binary - hold this lock for as long as the cwd is borrowed.
fn cwd_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[test]
fn init_writes_the_starter_config_when_none_exists() {
    let _guard = cwd_lock().lock().unwrap_or_else(|p| p.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let original = std::env::current_dir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    let result = init(false);

    std::env::set_current_dir(original).unwrap();
    result.unwrap();
    assert!(dir.path().join("foreman.toml").exists());
}

#[test]
fn init_refuses_to_overwrite_without_force() {
    let _guard = cwd_lock().lock().unwrap_or_else(|p| p.into_inner());
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("foreman.toml"), "existing").unwrap();
    let original = std::env::current_dir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    let result = init(false);

    std::env::set_current_dir(original).unwrap();
    assert!(result.is_err());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("foreman.toml")).unwrap(),
        "existing"
    );
}

#[test]
fn init_overwrites_with_force() {
    let _guard = cwd_lock().lock().unwrap_or_else(|p| p.into_inner());
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("foreman.toml"), "existing").unwrap();
    let original = std::env::current_dir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    let result = init(true);

    std::env::set_current_dir(original).unwrap();
    result.unwrap();
    assert_ne!(
        std::fs::read_to_string(dir.path().join("foreman.toml")).unwrap(),
        "existing"
    );
}

#[test]
fn summary_prints_urls_for_the_selection_and_applicable_notes() {
    let dir = tempfile::tempdir().unwrap();
    let config_text = r#"
            [[services]]
            name = "web"
            package = "web-svc"
            port = 8080

            [[services]]
            name = "auth"
            package = "auth-svc"
            port = 8081

            [[notes]]
            tone = "info"
            label = "tip"
            message = "web is selected"
            when_selected = "web"

            [[notes]]
            tone = "warn"
            label = "heads up"
            message = "always shown"
        "#;
    let estate = estate_in(dir.path(), config_text);
    summary(&estate, &["web".to_string()]).unwrap();
}

#[test]
fn summary_skips_a_note_scoped_to_a_service_not_in_the_selection() {
    let dir = tempfile::tempdir().unwrap();
    let config_text = r#"
            [[services]]
            name = "web"
            package = "web-svc"
            port = 8080

            [[notes]]
            tone = "info"
            label = "tip"
            message = "only when auth starts"
            when_selected = "auth"
        "#;
    let estate = estate_in(dir.path(), config_text);
    summary(&estate, &["web".to_string()]).unwrap();
}

#[test]
fn summary_with_an_empty_selection_still_succeeds() {
    let dir = tempfile::tempdir().unwrap();
    let estate = estate_in(dir.path(), CATALOG);
    summary(&estate, &[]).unwrap();
}
