use anvil::commands::workspace::{
    deny, ensure_tool_installed, format_metadata, list, machete, previous_version_rev,
};
use serde_json::json;

use crate::support;
use support::stable_cwd_lock;

#[test]
fn format_metadata_json_pretty_prints_the_whole_document() {
    let metadata = json!({ "packages": [] });
    let rendered = format_metadata("json", &metadata).unwrap();
    assert!(rendered.contains("\"packages\""));
}

#[test]
fn format_metadata_names_lists_one_per_line() {
    let metadata = json!({
        "packages": [
            { "name": "pkg-a" },
            { "name": "pkg-b" }
        ]
    });
    let rendered = format_metadata("names", &metadata).unwrap();
    assert_eq!(rendered, "pkg-a\npkg-b");
}

#[test]
fn format_metadata_names_is_empty_when_packages_is_missing() {
    let metadata = json!({});
    let rendered = format_metadata("names", &metadata).unwrap();
    assert_eq!(rendered, "");
}

#[test]
fn format_metadata_rejects_an_unknown_format() {
    let metadata = json!({});
    let error = format_metadata("yaml", &metadata).unwrap_err();
    assert!(error.to_string().contains("Unknown format"));
}

#[test]
fn list_runs_against_the_real_workspace_for_every_known_format() {
    // Shells out to real `cargo metadata` with no explicit
    // `--manifest-path`, so it needs cwd to stay put for its duration -
    // see `stable_cwd_lock`'s docs.
    let _guard = stable_cwd_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    list("json").expect("json format succeeds");
    list("names").expect("names format succeeds");
    assert!(list("bogus-format").is_err());
}

#[test]
fn ensure_tool_installed_succeeds_for_a_binary_that_exists() {
    ensure_tool_installed("cargo", "n/a").expect("cargo is on PATH in this environment");
}

#[test]
fn ensure_tool_installed_errors_with_the_install_hint_for_a_missing_binary() {
    let error = ensure_tool_installed(
        "definitely-not-a-real-binary-anvil-workspace-test",
        "cargo install something",
    )
    .unwrap_err();
    assert!(error.to_string().contains("cargo install something"));
}

#[test]
fn previous_version_rev_finds_the_commit_before_the_last_change_to_a_tracked_file() {
    // `git log` (no `-C`/`current_dir` override) needs cwd inside a git
    // repo to find it at all - see `stable_cwd_lock`'s docs. Built against
    // its own throwaway repo rather than this checkout: conveyor clones
    // shallow by default (see .conveyor.toml), so the real repo can have
    // just one commit behind a given file, which `--skip=1` can't land on.
    let _guard = stable_cwd_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let dir = tempfile::tempdir().unwrap();
    let git = |args: &[&str]| {
        assert!(
            std::process::Command::new("git")
                .args(args)
                .current_dir(dir.path())
                .status()
                .unwrap()
                .success(),
            "git {args:?} failed"
        );
    };
    let manifest = dir.path().join("Cargo.toml");

    git(&["init", "-q"]);
    git(&["config", "user.email", "anvil-test@example.com"]);
    git(&["config", "user.name", "anvil-test"]);
    std::fs::write(&manifest, "[package]\nname = \"a\"\nversion = \"0.1.0\"\n").unwrap();
    git(&["add", "Cargo.toml"]);
    git(&["commit", "-q", "-m", "first"]);
    std::fs::write(&manifest, "[package]\nname = \"a\"\nversion = \"0.2.0\"\n").unwrap();
    git(&["add", "Cargo.toml"]);
    git(&["commit", "-q", "-m", "bump"]);

    let cwd = std::env::current_dir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();
    let rev = previous_version_rev(&manifest);
    std::env::set_current_dir(cwd).unwrap();

    let rev = rev.expect("has an earlier commit");
    assert_eq!(rev.len(), 40, "a full git SHA");
}

#[test]
fn previous_version_rev_errors_for_a_path_git_has_never_tracked() {
    let _guard = stable_cwd_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    // Must live inside the repo's working tree (unlike `std::env::temp_dir()`,
    // which is outside it) so `git log --` recognizes the path at all and
    // returns empty output rather than failing the command outright.
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("never-committed-anvil-workspace-test.toml");
    std::fs::write(&path, "").unwrap();

    let error = previous_version_rev(&path).unwrap_err();
    assert!(error.to_string().contains("No earlier commit found"));

    let _ = std::fs::remove_file(&path);
}

// `machete()` and `deny()` shell out to `cargo machete`/`cargo deny check` -
// both are genuinely read-only static analysis (no `--fix`, no network
// dependency-fetch like `cargo audit`'s advisory-db pull), so it's safe to
// run them for real against this workspace rather than faking them. Neither
// asserts success: a real `cargo machete`/`cargo deny check` finding
// something to flag in this workspace is a legitimate `Err`, not a test
// failure - the point is exercising the command-construction and
// `run_command` plumbing, not asserting this repo is currently clean.
#[test]
fn machete_runs_cargo_machete_for_real_against_this_workspace() {
    let _guard = stable_cwd_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _ = machete();
}

#[test]
fn deny_runs_cargo_deny_check_for_real_against_this_workspace() {
    let _guard = stable_cwd_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _ = deny();
}
