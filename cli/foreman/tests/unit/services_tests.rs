use foreman::config::Config;
use foreman::estate::Estate;
use foreman::process::{self};
use foreman::services::*;
use foreman::vars;
use std::path::Path;

/// Root is a real, empty temp dir - `ensure_cert`'s tests need real files
/// on disk, and everything else here only needs `pid_file`/`run_dir` to
/// resolve to *some* writable path.
fn estate_in(root: &Path, text: &str) -> Estate {
    let config: Config = toml::from_str(text).unwrap();
    let vars = vars::resolve(root, &config.vars).unwrap();
    Estate {
        root: root.to_path_buf(),
        config_path: root.join("foreman.toml"),
        config,
        vars,
    }
}

const ONE_SERVICE: &str = r#"
        [[services]]
        name = "web"
        package = "web-svc"
        port = 8080
    "#;

#[test]
fn is_running_and_pid_are_none_without_a_pid_file() {
    let dir = tempfile::tempdir().unwrap();
    let estate = estate_in(dir.path(), ONE_SERVICE);
    assert!(!is_running(&estate, "web"));
    assert_eq!(pid(&estate, "web"), None);
}

#[test]
fn is_running_and_pid_are_none_for_a_pid_file_naming_a_dead_process() {
    let dir = tempfile::tempdir().unwrap();
    let estate = estate_in(dir.path(), ONE_SERVICE);
    estate.ensure_dirs().unwrap();
    process::write_pid(&estate.pid_file("web"), i32::MAX).unwrap();

    assert!(!is_running(&estate, "web"));
    assert_eq!(pid(&estate, "web"), None);
}

#[test]
fn pid_is_some_for_a_pid_file_naming_a_live_process() {
    let dir = tempfile::tempdir().unwrap();
    let estate = estate_in(dir.path(), ONE_SERVICE);
    estate.ensure_dirs().unwrap();
    let ours = std::process::id() as i32;
    process::write_pid(&estate.pid_file("web"), ours).unwrap();

    assert!(is_running(&estate, "web"));
    assert_eq!(pid(&estate, "web"), Some(ours));
}

#[test]
fn ensure_cert_is_a_no_op_when_the_service_does_not_borrow_one() {
    let dir = tempfile::tempdir().unwrap();
    let estate = estate_in(dir.path(), ONE_SERVICE);
    ensure_cert(&estate, "web").unwrap();
}

#[test]
fn ensure_cert_skips_when_the_cert_files_already_exist_in_the_workdir() {
    let dir = tempfile::tempdir().unwrap();
    let config_text = r#"
            [[services]]
            name = "web"
            package = "web-svc"
            port = 8080
            cert_from = "shared/cert"
            workdir = "services/web"
        "#;
    let estate = estate_in(dir.path(), config_text);
    let resolved = estate.resolve("web").unwrap();
    std::fs::create_dir_all(&resolved.workdir).unwrap();
    std::fs::write(resolved.workdir.join("cert.pem"), "cert").unwrap();
    std::fs::write(resolved.workdir.join("key.pem"), "key").unwrap();

    ensure_cert(&estate, "web").unwrap();
    // Still there, untouched - not replaced with a (missing) symlink.
    assert!(resolved.workdir.join("cert.pem").is_file());
}

#[test]
fn ensure_cert_warns_without_erroring_when_the_source_has_no_cert_either() {
    let dir = tempfile::tempdir().unwrap();
    let config_text = r#"
            [[services]]
            name = "web"
            package = "web-svc"
            port = 8080
            cert_from = "shared/cert"
        "#;
    let estate = estate_in(dir.path(), config_text);
    ensure_cert(&estate, "web").unwrap();
}

#[test]
fn ensure_cert_links_from_the_source_when_the_workdir_is_missing_it() {
    let dir = tempfile::tempdir().unwrap();
    let config_text = r#"
            [[services]]
            name = "web"
            package = "web-svc"
            port = 8080
            cert_from = "shared/cert"
            workdir = "services/web"
        "#;
    let estate = estate_in(dir.path(), config_text);
    let resolved = estate.resolve("web").unwrap();
    let source_dir = estate.path("shared/cert");
    std::fs::create_dir_all(&source_dir).unwrap();
    std::fs::write(source_dir.join("cert.pem"), "cert").unwrap();
    std::fs::write(source_dir.join("key.pem"), "key").unwrap();
    std::fs::create_dir_all(&resolved.workdir).unwrap();

    ensure_cert(&estate, "web").unwrap();

    assert!(resolved.workdir.join("cert.pem").is_symlink());
    assert!(resolved.workdir.join("key.pem").is_symlink());
}

#[test]
fn build_is_a_no_op_when_the_service_has_no_build_command() {
    let dir = tempfile::tempdir().unwrap();
    let config_text = r#"
            [[services]]
            name = "web"
            package = "web-svc"
            port = 8080
            build = []
        "#;
    let estate = estate_in(dir.path(), config_text);
    let resolved = estate.resolve("web").unwrap();
    build(&resolved, &estate.root).unwrap();
}

#[test]
fn build_runs_the_configured_command_and_errors_on_failure() {
    let dir = tempfile::tempdir().unwrap();
    let config_text = r#"
            [build]
            command = ["false"]

            [[services]]
            name = "web"
            package = "web-svc"
            port = 8080
        "#;
    let estate = estate_in(dir.path(), config_text);
    let resolved = estate.resolve("web").unwrap();
    assert!(build(&resolved, &estate.root).is_err());
}

#[test]
fn stop_reports_not_running_for_a_service_with_no_pid_file() {
    let dir = tempfile::tempdir().unwrap();
    let estate = estate_in(dir.path(), ONE_SERVICE);
    estate.ensure_dirs().unwrap();
    stop(&estate, &["web".to_string()]).unwrap();
}

#[test]
fn stop_terminates_a_service_with_a_live_pid_and_removes_the_pid_file() {
    let dir = tempfile::tempdir().unwrap();
    let estate = estate_in(dir.path(), ONE_SERVICE);
    estate.ensure_dirs().unwrap();

    let mut child = std::process::Command::new("sleep")
        .arg("30")
        .spawn()
        .unwrap();
    let child_pid = child.id() as i32;
    process::write_pid(&estate.pid_file("web"), child_pid).unwrap();

    stop(&estate, &["web".to_string()]).unwrap();

    assert!(!estate.pid_file("web").exists());
    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn report_strays_is_a_no_op_without_configured_warnings() {
    let dir = tempfile::tempdir().unwrap();
    let estate = estate_in(dir.path(), ONE_SERVICE);
    report_strays(&estate).unwrap();
}

#[test]
fn report_strays_is_quiet_when_the_pattern_matches_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let config_text = r#"
            [[services]]
            name = "web"
            package = "web-svc"
            port = 8080

            [[warnings]]
            name = "stray"
            pgrep = "foreman-coverage-test-pattern-matches-nothing-xyz"
            message = "still around: ${pids}"
        "#;
    let estate = estate_in(dir.path(), config_text);
    report_strays(&estate).unwrap();
}

#[test]
fn running_services_is_empty_when_nothing_has_a_pid_file() {
    let dir = tempfile::tempdir().unwrap();
    let estate = estate_in(dir.path(), ONE_SERVICE);
    assert!(running_services(&estate).is_empty());
}

#[test]
fn running_services_includes_a_service_with_a_live_pid() {
    let dir = tempfile::tempdir().unwrap();
    let estate = estate_in(dir.path(), ONE_SERVICE);
    estate.ensure_dirs().unwrap();
    process::write_pid(&estate.pid_file("web"), std::process::id() as i32).unwrap();

    assert_eq!(running_services(&estate), vec!["web".to_string()]);
}

#[test]
fn healthy_is_false_for_a_url_nothing_is_listening_on() {
    // Port 1 is reserved and never has a real server behind it, so `curl`
    // reliably fails fast without needing to configure a fake service at
    // all.
    assert!(!healthy("https://localhost:1/health"));
}

#[test]
fn start_spawns_a_real_process_and_times_out_when_it_never_answers_health_checks() {
    let dir = tempfile::tempdir().unwrap();
    let config_text = r#"
            [[services]]
            name = "web"
            package = "web-svc"
            port = 18080
            binary = "run.sh"
            build = []
            start_timeout_secs = 1
        "#;
    let estate = estate_in(dir.path(), config_text);
    estate.ensure_dirs().unwrap();

    let resolved = estate.resolve("web").unwrap();
    std::fs::create_dir_all(&resolved.workdir).unwrap();
    std::fs::write(&resolved.binary, "#!/bin/sh\nexec sleep 30\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&resolved.binary).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&resolved.binary, perms).unwrap();
    }

    // Nothing ever listens on port 18080, so `healthy()` never succeeds and
    // this exercises the `Wait::Timeout` branch - the process is left
    // running (matching production behaviour: still up, just not answering
    // yet), so it's found and killed via the pid file afterwards.
    let started = start(&estate, "web").expect("start");
    assert!(started);
    assert!(estate.pid_file("web").exists());

    let pid = process::read_pid(&estate.pid_file("web")).unwrap();
    process::kill(pid);
}

#[test]
fn run_pre_stop_hooks_is_a_no_op_when_the_service_is_not_running() {
    let dir = tempfile::tempdir().unwrap();
    let config_text = r#"
            [[services]]
            name = "web"
            package = "web-svc"
            port = 8080

            [[services.pre_stop]]
            shell = "true"
        "#;
    let estate = estate_in(dir.path(), config_text);
    // No pid file at all, so `is_running` is false and the hook never runs.
    run_pre_stop_hooks(&estate, "web").unwrap();
}

#[test]
fn run_pre_stop_hooks_runs_the_configured_shell_command_for_a_running_service() {
    let dir = tempfile::tempdir().unwrap();
    let config_text = r#"
            [[services]]
            name = "web"
            package = "web-svc"
            port = 8080

            [[services.pre_stop]]
            description = "flushing"
            shell = "true"
            timeout_secs = 5
        "#;
    let estate = estate_in(dir.path(), config_text);
    estate.ensure_dirs().unwrap();
    process::write_pid(&estate.pid_file("web"), std::process::id() as i32).unwrap();

    run_pre_stop_hooks(&estate, "web").unwrap();
}
