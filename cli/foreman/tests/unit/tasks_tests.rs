use foreman::config::Config;
use foreman::config::{Role, SelectionMode, Task};
use foreman::estate::Estate;
use foreman::tasks::*;
use foreman::vars;

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

#[test]
fn containers_for_role_collects_without_duplicates_across_tasks() {
    let dir = tempfile::tempdir().unwrap();
    let estate = estate_in(
        dir.path(),
        r#"
                [tasks.migrate-a]
                role = "migrate"
                containers = ["db"]
                command = ["true"]

                [tasks.migrate-b]
                role = "migrate"
                containers = ["db", "cache"]
                command = ["true"]
            "#,
    );
    let containers = containers_for_role(&estate, foreman::config::Role::Migrate);
    assert_eq!(containers, vec!["db".to_string(), "cache".to_string()]);
}

#[test]
fn containers_for_role_is_empty_when_no_task_has_that_role() {
    let dir = tempfile::tempdir().unwrap();
    let estate = estate_in(dir.path(), "");
    assert!(containers_for_role(&estate, foreman::config::Role::Reset).is_empty());
}

#[test]
fn run_named_errors_for_an_unknown_task() {
    let dir = tempfile::tempdir().unwrap();
    let estate = estate_in(dir.path(), "");
    let error = run_named(&estate, "no-such-task", &[]).unwrap_err();
    assert!(error.to_string().contains("unknown task"));
}

#[test]
fn run_named_runs_a_known_task() {
    let dir = tempfile::tempdir().unwrap();
    let estate = estate_in(
        dir.path(),
        r#"
                [tasks.hello]
                command = ["true"]
            "#,
    );
    run_named(&estate, "hello", &[]).unwrap();
}

#[test]
fn run_bails_when_the_command_exits_with_failure() {
    let dir = tempfile::tempdir().unwrap();
    let estate = estate_in(dir.path(), "");
    let task = Task {
        role: Role::Manual,
        description: None,
        containers: Vec::new(),
        build: Vec::new(),
        command: vec!["false".to_string()],
        env: Default::default(),
        workdir: None,
        each_selected: Vec::new(),
        selection: SelectionMode::Never,
        stop_services: false,
        warn: None,
        done: None,
    };
    let error = run(&estate, "fails", &task, &[]).unwrap_err();
    assert!(error.to_string().contains("failed"));
}

#[test]
fn run_bails_when_the_build_step_fails() {
    let dir = tempfile::tempdir().unwrap();
    let estate = estate_in(dir.path(), "");
    let task = Task {
        role: Role::Manual,
        description: None,
        containers: Vec::new(),
        build: vec!["false".to_string()],
        command: vec!["true".to_string()],
        env: Default::default(),
        workdir: None,
        each_selected: Vec::new(),
        selection: SelectionMode::Never,
        stop_services: false,
        warn: None,
        done: None,
    };
    let error = run(&estate, "build-fails", &task, &[]).unwrap_err();
    assert!(error.to_string().contains("build step failed"));
}

#[test]
fn run_bails_on_an_empty_command() {
    let dir = tempfile::tempdir().unwrap();
    let estate = estate_in(dir.path(), "");
    let task = Task {
        role: Role::Manual,
        description: None,
        containers: Vec::new(),
        build: Vec::new(),
        command: Vec::new(),
        env: Default::default(),
        workdir: None,
        each_selected: Vec::new(),
        selection: SelectionMode::Never,
        stop_services: false,
        warn: None,
        done: None,
    };
    let error = run(&estate, "empty", &task, &[]).unwrap_err();
    assert!(error.to_string().contains("empty command"));
}

#[test]
fn run_substitutes_the_selection_placeholder_into_the_command() {
    let dir = tempfile::tempdir().unwrap();
    let estate = estate_in(
        dir.path(),
        r#"
                [[services]]
                name = "web"
                package = "web-svc"
                port = 8080
            "#,
    );
    let task = Task {
        role: Role::Manual,
        description: None,
        containers: Vec::new(),
        build: Vec::new(),
        command: vec![
            "sh".to_string(),
            "-c".to_string(),
            SELECTION_PLACEHOLDER.to_string(),
        ],
        env: Default::default(),
        workdir: None,
        each_selected: vec!["true".to_string()],
        selection: SelectionMode::Always,
        stop_services: false,
        warn: None,
        // `done` expands against the task-level scope, not the
        // per-selected-service one `each_selected` uses - `${service}`
        // isn't in scope here.
        done: Some("all done".to_string()),
    };
    run(&estate, "scoped", &task, &["web".to_string()]).unwrap();
}

#[test]
fn run_reports_the_warning_and_stops_services_first_when_configured() {
    let dir = tempfile::tempdir().unwrap();
    let estate = estate_in(
        dir.path(),
        r#"
                [[services]]
                name = "web"
                package = "web-svc"
                port = 8080
            "#,
    );
    estate.ensure_dirs().unwrap();
    let task = Task {
        role: Role::Manual,
        description: None,
        containers: Vec::new(),
        build: Vec::new(),
        command: vec!["true".to_string()],
        env: Default::default(),
        workdir: None,
        each_selected: Vec::new(),
        selection: SelectionMode::Never,
        stop_services: true,
        warn: Some("stopping first".to_string()),
        done: None,
    };
    // Nothing is actually running (no pid files), so `stop_services`
    // exercises the "not running" branch rather than killing anything.
    run(&estate, "with-warning", &task, &[]).unwrap();
}

#[test]
fn run_errors_for_an_unknown_container() {
    let dir = tempfile::tempdir().unwrap();
    let estate = estate_in(dir.path(), "");
    let task = Task {
        role: Role::Manual,
        description: None,
        containers: vec!["no-such-container".to_string()],
        build: Vec::new(),
        command: vec!["true".to_string()],
        env: Default::default(),
        workdir: None,
        each_selected: Vec::new(),
        selection: SelectionMode::Never,
        stop_services: false,
        warn: None,
        done: None,
    };
    let error = run(&estate, "bad-container", &task, &[]).unwrap_err();
    assert!(error.to_string().contains("no-such-container"));
}
