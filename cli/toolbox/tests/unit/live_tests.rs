//! Exercises the wrappers around `cargo install --list` / `cargo search`.
//! These shell out for real: `cargo install --list` only reads local state,
//! and this workspace's sandbox has network access to the `ennor` registry
//! (the same one `forge-toolbox` targets), so both are safe and fast enough
//! to run as part of the normal suite rather than being gated behind an env
//! var the way the Postgres-backed service tests are.

use forge_toolbox::{
    MONITORED_CRATES, collect_statuses, fetch_latest_registry_version, installed_versions,
    refresh_app_state, toolbox_note,
};
use std::collections::HashMap;

#[test]
fn installed_versions_reads_the_local_cargo_install_list() {
    // Whatever is or isn't installed on this machine, the call itself must
    // succeed and return a plain map.
    let installed = installed_versions().expect("cargo install --list should succeed");
    assert!(installed.len() < 10_000, "sanity bound, not a real limit");
}

#[test]
fn fetch_latest_registry_version_finds_a_known_package() {
    // anvil is this workspace's own build tool and is always published to
    // the ennor registry, so this is a stable fixture rather than a flaky
    // external dependency.
    let version = fetch_latest_registry_version("anvil").expect("registry search should succeed");
    assert!(version.is_some());
}

#[test]
fn fetch_latest_registry_version_returns_none_for_an_unpublished_name() {
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
