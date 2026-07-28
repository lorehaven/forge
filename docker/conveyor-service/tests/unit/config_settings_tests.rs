//! Unit tests for `config/settings.rs`.
//!
//! `positive` reads the process environment, which every test in this binary
//! shares. Each test therefore uses a key of its own rather than a common one -
//! two tests setting the same variable would pass or fail depending on which
//! thread got there first.

use conveyor_service::config::settings::positive;
use conveyor_service::config::{ConveyorConfig, ExecutorKind};

#[test]
fn executor_defaults_to_native() {
    assert_eq!(ExecutorKind::default(), ExecutorKind::Native);
    assert_eq!(ConveyorConfig::default().executor, ExecutorKind::Native);
}

#[test]
fn executor_parses_what_it_renders() {
    for kind in [
        ExecutorKind::Native,
        ExecutorKind::Kubernetes,
        ExecutorKind::Mock,
    ] {
        assert_eq!(ExecutorKind::parse(&kind.to_string()), kind);
    }
}

#[test]
fn executor_accepts_the_obvious_abbreviation_and_ignores_case() {
    assert_eq!(ExecutorKind::parse("k8s"), ExecutorKind::Kubernetes);
    assert_eq!(
        ExecutorKind::parse("  KUBERNETES "),
        ExecutorKind::Kubernetes
    );
}

#[test]
fn an_unset_or_unknown_executor_falls_back_to_native() {
    assert_eq!(ExecutorKind::parse(""), ExecutorKind::Native);
    assert_eq!(ExecutorKind::parse("docker"), ExecutorKind::Native);
}

#[test]
fn forks_are_not_buildable_by_default() {
    // The native executor would run a fork's pipeline with this service's
    // privileges. If this default ever flips, that happens silently.
    assert!(!ConveyorConfig::default().allow_fork_pr);
}

#[test]
fn positive_keeps_the_default_when_unset() {
    assert_eq!(positive("CONVEYOR_TEST_UNSET_KEY", 7), 7);
}

#[test]
fn positive_reads_a_valid_value() {
    unsafe { std::env::set_var("CONVEYOR_TEST_VALID", "12") };
    assert_eq!(positive("CONVEYOR_TEST_VALID", 7), 12);
}

#[test]
fn positive_rejects_zero() {
    // Zero workers, or a zero-second timeout, is a service that silently does
    // nothing - worse than one that reports a bad configuration.
    unsafe { std::env::set_var("CONVEYOR_TEST_ZERO", "0") };
    assert_eq!(positive("CONVEYOR_TEST_ZERO", 7), 7);
}

#[test]
fn positive_rejects_garbage_and_negatives() {
    unsafe { std::env::set_var("CONVEYOR_TEST_GARBAGE", "lots") };
    assert_eq!(positive("CONVEYOR_TEST_GARBAGE", 7), 7);

    unsafe { std::env::set_var("CONVEYOR_TEST_NEGATIVE", "-3") };
    assert_eq!(positive("CONVEYOR_TEST_NEGATIVE", 7), 7);
}

#[test]
fn positive_tolerates_surrounding_whitespace() {
    unsafe { std::env::set_var("CONVEYOR_TEST_PADDED", "  4  ") };
    assert_eq!(positive("CONVEYOR_TEST_PADDED", 7), 4);
}
