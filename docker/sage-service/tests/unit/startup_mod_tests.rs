//! `startup/mod.rs`'s `health_check_url`. `init_tracing` isn't tested here -
//! it initializes a process-global tracing subscriber, which can only be
//! done once per process and isn't something a test should own.

use crate::env_support::env_lock;
use sage_service::startup::health_check_url;

#[test]
fn health_check_url_defaults_to_the_in_cluster_switchboard_address() {
    let _guard = env_lock().lock().unwrap_or_else(|p| p.into_inner());
    unsafe { std::env::remove_var("SWITCHBOARD_URL") };

    assert_eq!(
        health_check_url(),
        "http://switchboard-service:8080/health/ready"
    );
}

#[test]
fn health_check_url_honors_switchboard_url_and_trims_a_trailing_slash() {
    let _guard = env_lock().lock().unwrap_or_else(|p| p.into_inner());
    unsafe { std::env::set_var("SWITCHBOARD_URL", "http://custom-host:9000/") };

    assert_eq!(health_check_url(), "http://custom-host:9000/health/ready");

    unsafe { std::env::remove_var("SWITCHBOARD_URL") };
}
