//! `anvil release` from start to finish in the throwaway workspace of `flow_support`, with `cargo`
//! shimmed so nothing is published.

use crate::flow_support::{Workspace, git};
use std::fs;

#[quench_test::test]
fn a_dry_run_shows_the_graph_and_changes_nothing() {
    let w = Workspace::new();
    let out = w.run(&["release", "--all", "--dry-run"], "");
    assert!(out.ok, "{}", out.text);
    assert!(
        out.text.contains('a') && out.text.contains('b'),
        "{}",
        out.text
    );
    assert!(w.tags().is_empty());
    assert!(w.cargo_calls().is_empty());
}

#[quench_test::test]
fn a_first_release_publishes_in_dependency_order_and_tags_what_it_shipped() {
    let w = Workspace::new();
    let out = w.run(&["release", "--all"], "yes\n");
    assert!(out.ok, "{}", out.text);
    assert!(
        out.text.contains("Release completed successfully"),
        "{}",
        out.text
    );

    // never released, so the current version is what ships - nothing is bumped or committed
    let calls = w.cargo_calls();
    assert_eq!(calls.len(), 2, "{calls:?}");
    assert!(
        calls[0].contains("publish --registry test --package a"),
        "{calls:?}"
    );
    assert!(
        calls[1].contains("publish --registry test --package b"),
        "{calls:?}"
    );
    assert_eq!(w.tags(), ["a-v0.1.0", "b-v0.1.0"]);
    assert_eq!(
        w.remote_tags(),
        ["a-v0.1.0", "b-v0.1.0"],
        "tags are pushed as they are made"
    );
    assert_eq!(git(&w.work(), &["log", "--oneline"]).lines().count(), 1);

    // nothing has changed since, so a second run has nothing to do
    let again = w.run(&["release", "--all"], "yes\n");
    assert!(again.ok, "{}", again.text);
    assert!(!again.text.contains("Release completed"), "{}", again.text);
    assert_eq!(w.cargo_calls().len(), 2);
}

#[quench_test::test]
fn a_changed_package_is_bumped_committed_built_published_and_tagged() {
    let w = Workspace::new();
    assert!(w.run(&["release", "--all"], "yes\n").ok);
    w.change_crate_a();

    let out = w.run(&["release", "--all"], "yes\n");
    assert!(out.ok, "{}", out.text);

    // a changed, and b depends on a, so both ship a new patch version
    let manifest = |name: &str| {
        fs::read_to_string(w.work().join(format!("crates/{name}/Cargo.toml"))).unwrap()
    };
    assert!(
        manifest("a").contains("version = \"0.1.1\""),
        "{}",
        manifest("a")
    );
    assert!(
        manifest("b").contains("version = \"0.1.1\""),
        "{}",
        manifest("b")
    );
    let log = git(&w.work(), &["log", "--format=%s", "-n", "1"]);
    assert_eq!(
        log.trim(),
        "release: bump package versions (a v0.1.1, b v0.1.1)"
    );
    // the version commit was pushed
    assert_eq!(
        git(&w.work(), &["rev-parse", "HEAD"]),
        git(&w.work(), &["rev-parse", "origin/main"])
    );

    let calls = w.cargo_calls();
    let build = calls
        .iter()
        .find(|c| c.starts_with("build"))
        .expect("a build of the bumped packages");
    assert!(
        build.contains("--package a") && build.contains("--package b"),
        "{calls:?}"
    );
    let published = calls.iter().filter(|c| c.starts_with("publish")).count();
    assert_eq!(published, 4, "{calls:?}");
    assert_eq!(w.tags(), ["a-v0.1.0", "a-v0.1.1", "b-v0.1.0", "b-v0.1.1"]);
    assert_eq!(w.remote_tags(), w.tags());
}

#[quench_test::test]
fn a_release_can_be_declined_or_limited_to_one_package() {
    let w = Workspace::new();
    let declined = w.run(&["release", "--all"], "no\n");
    assert!(
        declined.ok && declined.text.contains("Release cancelled."),
        "{}",
        declined.text
    );
    let eof = w.run(&["release", "--all"], "");
    assert!(
        eof.ok && eof.text.contains("Release cancelled."),
        "{}",
        eof.text
    );
    assert!(w.tags().is_empty() && w.cargo_calls().is_empty());

    let one = w.run(&["release", "--package", "a"], "y\n");
    assert!(one.ok, "{}", one.text);
    assert_eq!(w.tags(), ["a-v0.1.0"]);
    assert!(!w.run(&["release", "--package", "ghost"], "y\n").ok);
}

#[quench_test::test]
fn a_failed_publish_stops_the_release_but_keeps_the_tags_of_what_shipped() {
    let w = Workspace::new();
    let out = w.run_failing(&["release", "--all"], "yes\n", "b");
    assert!(!out.ok, "{}", out.text);
    assert!(out.text.contains("publish (b)"), "{}", out.text);
    // a is out in the registry, and git says so; b is not, and git does not
    assert_eq!(w.tags(), ["a-v0.1.0"]);
    assert_eq!(w.remote_tags(), ["a-v0.1.0"]);
}

#[quench_test::test]
fn a_release_refuses_to_clobber_an_existing_tag() {
    let w = Workspace::new();
    assert!(w.run(&["release", "--all"], "yes\n").ok);
    w.change_crate_a();
    // a leftover tag on an older commit: `a` has changed since it, so 0.1.1 is planned, and taken
    git(&w.work(), &["tag", "a-v0.1.1", "HEAD~1"]);

    let out = w.run(&["release", "--all"], "yes\n");
    assert!(!out.ok, "{}", out.text);
    assert!(out.text.contains("a-v0.1.1"), "{}", out.text);
    assert_eq!(w.cargo_calls().len(), 2, "nothing more was published");
}
