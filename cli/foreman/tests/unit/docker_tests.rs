use foreman::config::Container;
use foreman::docker::*;

/// A container name distinctive enough that no real container - on this
/// machine or any other - could ever collide with it, so `is_running`
/// against the real `docker` binary always takes the "not found" branch.
fn nonexistent(name: &str) -> Container {
    Container {
        name: name.to_string(),
        image: "ghcr.io/example/does-not-exist".to_string(),
        container_name: Some(format!("foreman-coverage-test-{name}-does-not-exist")),
        ports: Vec::new(),
        env: Default::default(),
        args: Vec::new(),
        ready: Vec::new(),
        ready_timeout_secs: 1,
        address: None,
    }
}

#[test]
fn is_running_is_false_for_a_container_that_does_not_exist() {
    assert!(!is_running(&nonexistent("is-running")));
}

#[test]
fn status_reports_not_running_without_panicking() {
    status(&nonexistent("status-missing"));
}

#[test]
fn status_reports_running_with_an_address_without_panicking() {
    let mut container = nonexistent("status-with-address");
    container.address = Some("localhost:5432".to_string());
    // Still not running (it doesn't exist), but exercises the address
    // formatting path the same way the "running" branch would.
    status(&container);
}

#[test]
fn stop_reports_not_running_for_a_container_that_does_not_exist() {
    stop(&nonexistent("stop-missing"));
}

#[test]
fn container_name_defaults_to_name_when_unset() {
    let mut container = nonexistent("default-name");
    container.container_name = None;
    assert_eq!(container.container_name(), "default-name");
}

#[test]
fn container_name_uses_the_override_when_set() {
    let container = nonexistent("has-override");
    assert_eq!(
        container.container_name(),
        "foreman-coverage-test-has-override-does-not-exist"
    );
}

#[test]
fn require_docker_succeeds_when_docker_is_on_path() {
    // This sandbox has a real docker daemon on PATH (used elsewhere in this
    // workspace's own test setup), so this is safe to run for real.
    require_docker().expect("docker on PATH");
}

#[test]
fn wait_until_ready_with_no_readiness_probe_succeeds_immediately() {
    // `container.ready` is empty, so this never shells out to `docker exec`
    // at all - it just prints the ready message and returns.
    wait_until_ready(&nonexistent("wait-no-probe")).expect("no probe configured");
}

#[test]
fn wait_until_ready_with_an_address_and_no_probe_reports_it() {
    let mut container = nonexistent("wait-no-probe-with-address");
    container.address = Some("localhost:5432".to_string());
    wait_until_ready(&container).expect("no probe configured");
}

#[test]
fn wait_until_ready_times_out_when_the_probe_never_succeeds() {
    // `docker exec` against a container that does not exist always fails,
    // so this reliably exercises the "did not become ready" bail branch -
    // `ready_timeout_secs` is 1 in `nonexistent`, so this stays fast.
    let mut container = nonexistent("wait-probe-times-out");
    container.ready = vec!["true".to_string()];
    let err = wait_until_ready(&container).unwrap_err();
    assert!(err.to_string().contains("did not become ready"));
}
