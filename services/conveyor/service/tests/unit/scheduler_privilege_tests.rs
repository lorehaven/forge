//! Unit tests for `scheduler/privilege.rs`: who may run a privileged service.

use conveyor_service::domain::Trigger;
use conveyor_service::pipeline::{Readiness, Service};
use conveyor_service::scheduler::privilege::check;
use std::collections::BTreeMap;

fn service(name: &str, privileged: bool) -> Service {
    Service {
        name: name.to_string(),
        image: "docker:dind".to_string(),
        env: BTreeMap::new(),
        args: Vec::new(),
        ready: Readiness::Port(2375),
        privileged,
    }
}

fn repos(list: &[&str]) -> Vec<String> {
    list.iter().map(ToString::to_string).collect()
}

#[quench_test::test]
fn a_job_with_no_privileged_service_needs_no_permission() {
    assert!(check(&[], Trigger::PullRequest, "a/b", &[]).is_ok());
    assert!(check(&[service("db", false)], Trigger::PullRequest, "a/b", &[]).is_ok());
}

#[quench_test::test]
fn by_default_nobody_may() {
    for trigger in [Trigger::Push, Trigger::Manual, Trigger::Schedule] {
        let why = check(&[service("dind", true)], trigger, "lorehaven/forge", &[]).unwrap_err();
        assert!(
            why.contains("lorehaven/forge may not run")
                && why.contains("dind")
                && why.contains("CONVEYOR_PRIVILEGED_REPOS"),
            "{why}"
        );
    }
}

#[quench_test::test]
fn a_named_repository_may_on_every_trigger_but_a_pull_request() {
    let allowed = repos(&["lorehaven/forge", "other/thing"]);
    for trigger in [Trigger::Push, Trigger::Manual, Trigger::Schedule] {
        assert!(
            check(
                &[service("dind", true)],
                trigger,
                "lorehaven/forge",
                &allowed
            )
            .is_ok()
        );
    }
    assert!(
        check(
            &[service("dind", true)],
            Trigger::Push,
            "LoreHaven/Forge",
            &allowed
        )
        .is_ok(),
        "names are not case sensitive"
    );
    let why = check(
        &[service("dind", true)],
        Trigger::PullRequest,
        "lorehaven/forge",
        &allowed,
    )
    .unwrap_err();
    assert!(
        why.contains("a pull request cannot") && why.contains("dind"),
        "{why}"
    );
    // Another repository is still not allowed.
    assert!(
        check(
            &[service("dind", true)],
            Trigger::Push,
            "lorehaven/quench",
            &allowed
        )
        .is_err()
    );
}

#[quench_test::test]
fn a_star_allows_every_repository_but_still_not_a_pull_request() {
    let everyone = repos(&["*"]);
    assert!(
        check(
            &[service("dind", true)],
            Trigger::Push,
            "any/repo",
            &everyone
        )
        .is_ok()
    );
    assert!(
        check(
            &[service("dind", true)],
            Trigger::PullRequest,
            "any/repo",
            &everyone
        )
        .is_err()
    );
}

#[quench_test::test]
fn every_privileged_service_is_named() {
    let services = [
        service("dind", true),
        service("db", false),
        service("kvm", true),
    ];
    let why = check(&services, Trigger::Push, "a/b", &[]).unwrap_err();
    assert!(why.contains("dind, kvm") && !why.contains("db"), "{why}");
}
