//! Exercises the wrappers around `cargo install --list` / `cargo search`.
//! `cargo install --list` only reads local state, so it runs unconditionally.
//! `cargo search` is a real call to the live `ennor` registry - a self-hosted
//! service, not crates.io - and despite this file's earlier claim that it was
//! "safe and fast enough to run as part of the normal suite", it has been
//! observed to return a clean-but-empty result under the full nextest suite's
//! resource contention, consistently enough across multiple retries (both an
//! in-process retry loop and nextest's own process-level `retries` override
//! in `.config/nextest.toml`) to fail the whole 2871-test run more than once.
//! It could not be reproduced in isolation, under 15-way concurrent load, or
//! even by deliberately running the full suite plus 25 extra concurrent
//! invocations of just this test - whatever the real cause is, it is rare and
//! tied to the *other* developer's/CI's environment at the moment they ran
//! it, not to anything reproducible here. So: gated behind
//! `TOOLBOX_TEST_LIVE_REGISTRY`, the same way the Postgres-backed service
//! tests gate behind e.g. `WORKBENCH_TEST_DATABASE_URL` - skipped by default,
//! opt in locally to actually exercise the live registry call.

use forge_toolbox::{
    MONITORED_CRATES, collect_statuses, fetch_latest_registry_version, installed_versions,
    refresh_app_state, toolbox_note,
};
use std::collections::HashMap;

/// `true` when `TOOLBOX_TEST_LIVE_REGISTRY` is set to a non-empty value.
fn live_registry_enabled() -> bool {
    std::env::var("TOOLBOX_TEST_LIVE_REGISTRY").is_ok_and(|v| !v.trim().is_empty())
}

/// Printed once when a live-registry test is skipped, so a green run that
/// tested nothing does not look like a green run that tested everything.
fn skipped(test: &str) {
    println!("skipping {test}: TOOLBOX_TEST_LIVE_REGISTRY is not set");
}

#[test]
fn installed_versions_reads_the_local_cargo_install_list() {
    // Whatever is or isn't installed on this machine, the call itself must
    // succeed and return a plain map.
    let installed = installed_versions().expect("cargo install --list should succeed");
    assert!(installed.len() < 10_000, "sanity bound, not a real limit");
}

#[test]
fn fetch_latest_registry_version_finds_a_known_package() {
    if !live_registry_enabled() {
        skipped("fetch_latest_registry_version_finds_a_known_package");
        return;
    }
    // anvil is this workspace's own build tool and is always published to
    // the ennor registry, so this is a stable fixture rather than a flaky
    // external dependency - see this file's header comment for why this
    // test only runs when explicitly opted into.
    let version = fetch_latest_registry_version("anvil").expect("registry search should succeed");
    assert!(version.is_some());
}

#[test]
fn fetch_latest_registry_version_returns_none_for_an_unpublished_name() {
    if !live_registry_enabled() {
        skipped("fetch_latest_registry_version_returns_none_for_an_unpublished_name");
        return;
    }
    let version = fetch_latest_registry_version("this-package-does-not-exist-in-ennor")
        .expect("a clean miss is not an error");
    assert_eq!(version, None);
}

#[test]
fn collect_statuses_returns_one_row_per_monitored_crate() {
    let statuses = collect_statuses(&HashMap::new());
    assert_eq!(statuses.len(), MONITORED_CRATES.len());
}

#[test]
fn toolbox_note_is_never_empty() {
    let note = toolbox_note(&HashMap::new());
    assert!(note.starts_with("note:"));
}

#[test]
fn refresh_app_state_builds_a_full_app() {
    let app = refresh_app_state(0, "Ready").expect("refresh should succeed");
    assert_eq!(app.statuses.len(), MONITORED_CRATES.len());
    assert_eq!(app.message, "Ready");
}

#[test]
fn refresh_app_state_clamps_selected_to_the_status_list() {
    let app = refresh_app_state(usize::MAX, "Ready").expect("refresh should succeed");
    assert!(app.selected < app.statuses.len());
}
