use anvil::commands::run::{
    build_command, clear_echoed_hotkey, file_snapshot, resolve_package_dir,
    run_command_for_package, should_watch_file, should_watch_path, stop_child_if_running,
};
use std::path::Path;
use std::process::{Child, Command};
use std::thread;
use std::time::Duration;

use crate::support;
use support::stable_cwd_lock;

fn args_of(cmd: &Command) -> Vec<String> {
    cmd.get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect()
}

#[test]
fn build_command_without_a_package_builds_the_whole_workspace() {
    let cmd = build_command(None, true);
    assert_eq!(args_of(&cmd), vec!["build", "--all-features"]);
}

#[test]
fn build_command_with_a_package_scopes_to_it() {
    let cmd = build_command(Some("anvil"), false);
    assert_eq!(args_of(&cmd), vec!["build", "--package", "anvil"]);
}

#[test]
fn run_command_for_package_with_no_package_is_a_plain_cargo_run() {
    let cmd = run_command_for_package(None);
    assert_eq!(args_of(&cmd), vec!["run"]);
}

#[test]
fn run_command_for_package_sets_the_package_arg_and_working_dir() {
    // `resolve_package_dir` shells to real `cargo metadata` with no
    // explicit `--manifest-path` - see `stable_cwd_lock`'s docs.
    let _guard = stable_cwd_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let cmd = run_command_for_package(Some("anvil"));
    assert_eq!(args_of(&cmd), vec!["run", "--package", "anvil"]);
    // `resolve_package_dir("anvil")` should have found this very crate's
    // directory and set it as the child's cwd.
    assert!(cmd.get_current_dir().is_some());
}

#[test]
fn resolve_package_dir_finds_a_real_workspace_member() {
    let _guard = stable_cwd_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let dir = resolve_package_dir("anvil").expect("anvil is in this workspace");
    assert!(dir.ends_with("anvil"));
}

#[test]
fn resolve_package_dir_is_none_for_an_unknown_package() {
    let _guard = stable_cwd_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    assert!(resolve_package_dir("not-a-real-package-xyz").is_none());
}

#[test]
fn stop_child_if_running_kills_a_live_process_and_clears_the_slot() {
    let mut cmd = Command::new("sleep");
    cmd.arg("30");
    let child = cmd.spawn().expect("spawn sleep");
    let mut slot = Some(child);

    stop_child_if_running(&mut slot).expect("stop succeeds");
    assert!(slot.is_none());
}

#[test]
fn stop_child_if_running_is_a_no_op_for_an_already_exited_process() {
    let mut cmd = Command::new("true");
    let mut child = cmd.spawn().expect("spawn true");
    let _ = child.wait();
    let mut slot = Some(child);

    stop_child_if_running(&mut slot).expect("stop succeeds");
    assert!(slot.is_none());
}

#[test]
fn stop_child_if_running_is_a_no_op_for_an_empty_slot() {
    let mut slot: Option<Child> = None;
    stop_child_if_running(&mut slot).expect("stop succeeds");
    assert!(slot.is_none());
}

#[test]
fn clear_echoed_hotkey_does_not_panic() {
    clear_echoed_hotkey();
}

#[test]
fn should_watch_path_excludes_ignored_directories() {
    assert!(!should_watch_path(Path::new("/repo/target/debug/build.rs")));
    assert!(!should_watch_path(Path::new("/repo/.git/HEAD")));
    assert!(should_watch_path(Path::new("/repo/src/main.rs")));
}

#[test]
fn should_watch_file_matches_known_manifest_names_and_source_extensions() {
    assert!(should_watch_file(Path::new("Cargo.toml")));
    assert!(should_watch_file(Path::new("Cargo.lock")));
    assert!(should_watch_file(Path::new("rust-toolchain.toml")));
    assert!(should_watch_file(Path::new("src/lib.rs")));
    assert!(should_watch_file(Path::new("config.yaml")));
    assert!(!should_watch_file(Path::new("README.md")));
    assert!(!should_watch_file(Path::new("no-extension")));
}

#[test]
fn file_snapshot_tracks_watched_files_and_reacts_to_a_change() {
    // Not under `std::env::temp_dir()` (typically `/tmp`, whose "tmp"
    // path component is itself one of `should_watch_path`'s ignored
    // directory names) and not under this crate's own `target/` for the
    // same reason - a directory scoped to this test, directly inside
    // the crate, with a name that matches none of the ignore list.
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("anvil-run-test-scratch-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let watched = dir.join("lib.rs");
    std::fs::write(&watched, "fn main() {}").unwrap();
    std::fs::write(dir.join("README.md"), "ignored").unwrap();

    let first = file_snapshot(&dir).expect("snapshot succeeds");
    assert_eq!(first.len(), 1);
    assert!(first.keys().next().unwrap().ends_with("lib.rs"));

    thread::sleep(Duration::from_millis(10));
    std::fs::write(&watched, "fn main() { /* changed */ }").unwrap();
    let second = file_snapshot(&dir).expect("snapshot succeeds");
    assert_ne!(
        first, second,
        "rewriting the watched file should change its recorded mtime"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
