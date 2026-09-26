use anvil::commands::workspace::{
    MacheteFinding, deny, empty_api_baseline, ensure_tool_installed, format_metadata, list,
    machete, parse_machete_output, previous_version_rev,
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

    let rev = rev.expect("git log itself succeeds").expect("has an earlier commit");
    assert_eq!(rev.len(), 40, "a full git SHA");
}

#[test]
fn previous_version_rev_is_none_for_a_manifest_with_only_one_commit() {
    // The first-release case this whole fallback exists for: the manifest
    // has exactly one commit in its history (the one that created it), so
    // `--skip=1` has nothing left to land on.
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
    git(&["commit", "-q", "-m", "first release"]);

    let cwd = std::env::current_dir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();
    let rev = previous_version_rev(&manifest);
    std::env::set_current_dir(cwd).unwrap();

    assert!(rev.expect("git log itself succeeds").is_none());
}

#[test]
fn empty_api_baseline_writes_a_stub_crate_matching_the_package_name() {
    let dir = empty_api_baseline("some-package").expect("writes the stub crate");

    let manifest =
        std::fs::read_to_string(dir.path().join("Cargo.toml")).expect("stub Cargo.toml exists");
    assert!(manifest.contains("name = \"some-package\""), "{manifest}");

    let lib = std::fs::read_to_string(dir.path().join("src/lib.rs")).expect("stub src/lib.rs exists");
    assert_eq!(lib, "", "an empty API to diff the real crate against");
}

#[test]
fn previous_version_rev_is_none_for_a_path_git_has_never_tracked() {
    let _guard = stable_cwd_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    // Must live inside the repo's working tree (unlike `std::env::temp_dir()`,
    // which is outside it) so `git log --` recognizes the path at all and
    // returns empty output rather than failing the command outright.
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("never-committed-anvil-workspace-test.toml");
    std::fs::write(&path, "").unwrap();

    let rev = previous_version_rev(&path).expect("git log itself succeeds");
    assert!(rev.is_none(), "no commit has ever touched this path");

    let _ = std::fs::remove_file(&path);
}

// `machete()` and `deny()` shell out to `cargo machete`/`cargo deny check` -
// both are genuinely read-only static analysis (no `--fix`, no network
// dependency-fetch like `cargo audit`'s advisory-db pull), so it's safe to
// run them for real against this workspace rather than faking them. Neither
// asserts success: a real `cargo machete`/`cargo deny check` finding
// something to flag in this workspace is a legitimate `Err`, not a test
// failure - the point is exercising the command-construction and the
// run_command* plumbing, not asserting this repo is currently clean.
#[test]
fn machete_runs_cargo_machete_for_real_against_this_workspace() {
    let _guard = stable_cwd_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _ = machete(false);
}

#[test]
fn machete_json_runs_cargo_machete_for_real_against_this_workspace() {
    let _guard = stable_cwd_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _ = machete(true);
}

#[test]
fn deny_runs_cargo_deny_check_for_real_against_this_workspace() {
    let _guard = stable_cwd_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _ = deny(false);
}

#[test]
fn deny_json_runs_cargo_deny_check_for_real_against_this_workspace() {
    let _guard = stable_cwd_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _ = deny(true);
}

#[test]
fn parse_machete_output_reads_no_unused_dependencies_as_empty() {
    let findings = parse_machete_output(
        "cargo-machete didn't find any unused dependencies in this directory. Good job!\n",
    );
    assert!(findings.is_empty());
}

#[test]
fn parse_machete_output_reads_one_finding_per_crate_block() {
    let stdout = "cargo-machete found the following unused dependencies in this directory:\n\
        workbench-service -- ./docker/workbench-service/Cargo.toml:\n\
        \trustls\n\
        switchboard-service -- ./docker/switchboard-service/Cargo.toml:\n\
        \trustls\n\
        \tserde\n\
        \n\
        If you believe cargo-machete has detected an unused dependency incorrectly,\n\
        you can add the dependency to the list of dependencies to ignore.\n";

    let findings = parse_machete_output(stdout);

    assert_eq!(
        findings,
        vec![
            MacheteFinding {
                package: "workbench-service".to_string(),
                manifest: "./docker/workbench-service/Cargo.toml".to_string(),
                unused: vec!["rustls".to_string()],
            },
            MacheteFinding {
                package: "switchboard-service".to_string(),
                manifest: "./docker/switchboard-service/Cargo.toml".to_string(),
                unused: vec!["rustls".to_string(), "serde".to_string()],
            },
        ]
    );
}

#[test]
fn parse_machete_output_ignores_unrecognised_text() {
    assert!(parse_machete_output("Analyzing dependencies of crates in this directory...\n").is_empty());
}
