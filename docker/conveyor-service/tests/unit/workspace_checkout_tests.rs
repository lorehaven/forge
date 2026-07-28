//! Unit tests for `workspace/checkout.rs` and `workspace/mod.rs`.
//!
//! The checkout tests drive a real `git` against a repository built in a
//! temporary directory. That is the point: the thing worth testing is whether
//! the four commands conveyor issues actually produce the commit on disk, and
//! a mocked `git` would only confirm that the arguments match themselves.

use conveyor_service::workspace::checkout::{validate_ref, validate_sha, validate_url};
use conveyor_service::workspace::{CheckoutError, CheckoutRequest, Workspace, checkout};
use std::path::Path;
use std::process::Command;
use std::time::Duration;

// ---------------------------------------------------------------------------
// Argument validation
// ---------------------------------------------------------------------------

#[test]
fn a_real_sha_is_accepted() {
    assert!(validate_sha("0123456789abcdef0123456789abcdef01234567").is_ok());
    assert!(
        validate_sha("abc1234").is_ok(),
        "a short sha is still a sha"
    );
}

#[test]
fn a_sha_that_is_not_hexadecimal_is_rejected() {
    assert!(validate_sha("not-a-sha-at-all").is_err());
    assert!(validate_sha("0123456789abcdefg").is_err());
}

#[test]
fn a_sha_of_the_wrong_length_is_rejected() {
    assert!(validate_sha("abc12").is_err());
    assert!(validate_sha(&"a".repeat(65)).is_err());
    assert!(validate_sha("").is_err());
}

#[test]
fn a_sha_that_looks_like_an_option_is_rejected() {
    // It would not be hexadecimal either, but this is the case that matters.
    assert!(validate_sha("--upload-pack=/tmp/x").is_err());
}

#[test]
fn ordinary_refs_are_accepted() {
    for git_ref in [
        "refs/heads/master",
        "refs/heads/release/1.2",
        "refs/tags/v1.0.0",
        "master",
        "feature/JIRA-123_thing",
    ] {
        assert!(
            validate_ref(git_ref).is_ok(),
            "{git_ref} should be accepted"
        );
    }
}

#[test]
fn a_ref_that_git_would_read_as_an_option_is_rejected() {
    // The exploit this validation exists for: in a ref position,
    // `--upload-pack=/tmp/x` runs /tmp/x. The ref comes from a webhook body.
    let error = validate_ref("--upload-pack=/tmp/pwned").expect_err("must be rejected");
    assert!(error.to_string().contains("option"), "{error}");

    assert!(validate_ref("-x").is_err());
}

#[test]
fn refs_breaking_git_naming_rules_are_rejected() {
    for bad in [
        "",
        "has space",
        "has\ttab",
        "has\nnewline",
        "a..b",
        "a~1",
        "a^",
        "a:b",
        "a?b",
        "a*b",
        "a[b",
        "a\\b",
        "a//b",
        "/leading",
        "trailing/",
        "trailing.",
        "branch.lock",
        "branch@{0}",
    ] {
        assert!(validate_ref(bad).is_err(), "{bad:?} should be rejected");
    }
}

#[test]
fn a_clone_url_that_looks_like_an_option_is_rejected() {
    assert!(validate_url("--upload-pack=/tmp/x").is_err());
    assert!(validate_url("").is_err());
    assert!(validate_url("https://github.com/lorehaven/forge.git").is_ok());
    assert!(validate_url("file:///srv/git/thing").is_ok());
}

// ---------------------------------------------------------------------------
// Path containment
// ---------------------------------------------------------------------------

#[test]
fn a_path_inside_the_checkout_resolves() {
    let dir = tempfile::tempdir().expect("temp dir");
    std::fs::create_dir_all(dir.path().join("target/release")).expect("mkdir");
    std::fs::write(dir.path().join("target/release/thing"), b"x").expect("write");

    let workspace = Workspace::new(dir.path().to_path_buf());
    assert!(workspace.resolve("target/release/thing").is_some());
}

#[test]
fn a_resolved_path_can_actually_be_opened() {
    // `is_some()` is not enough. Joining an empty remainder used to append a
    // trailing separator, and `stat("/a/file/")` is ENOTDIR - so every existing
    // file resolved to something that could not be read.
    let dir = tempfile::tempdir().expect("temp dir");
    std::fs::write(dir.path().join("built"), b"contents").expect("write");

    let workspace = Workspace::new(dir.path().to_path_buf());
    let resolved = workspace.resolve("built").expect("resolves");

    let metadata = std::fs::metadata(&resolved).expect("the resolved path should be readable");
    assert!(metadata.is_file());
    assert_eq!(std::fs::read(&resolved).expect("read"), b"contents");
}

#[test]
fn a_path_that_climbs_out_of_the_checkout_is_refused() {
    // A pipeline can say `artifacts = ["../../etc/passwd"]`, and collecting it
    // would hand a repository author whatever the service account can read.
    let dir = tempfile::tempdir().expect("temp dir");
    let workspace = Workspace::new(dir.path().to_path_buf());

    assert!(workspace.resolve("../outside").is_none());
    assert!(workspace.resolve("../../etc/passwd").is_none());
    assert!(workspace.resolve("/etc/passwd").is_none());
}

#[test]
fn a_symlink_pointing_out_of_the_checkout_is_refused() {
    // Planted in the repository, this is how `..` gets past a textual check.
    let dir = tempfile::tempdir().expect("temp dir");
    let outside = tempfile::tempdir().expect("temp dir");
    std::fs::write(outside.path().join("secret"), b"x").expect("write");

    #[cfg(unix)]
    std::os::unix::fs::symlink(outside.path(), dir.path().join("escape")).expect("symlink");

    let workspace = Workspace::new(dir.path().to_path_buf());
    assert!(workspace.resolve("escape/secret").is_none());
}

#[test]
fn a_path_that_does_not_exist_yet_still_resolves() {
    // Artifacts are named before the build produces them.
    let dir = tempfile::tempdir().expect("temp dir");
    let workspace = Workspace::new(dir.path().to_path_buf());
    assert!(workspace.resolve("target/release/not-built-yet").is_some());
}

// ---------------------------------------------------------------------------
// Checking out a real repository
// ---------------------------------------------------------------------------

fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "conveyor tests")
        .env("GIT_AUTHOR_EMAIL", "tests@example.invalid")
        .env("GIT_COMMITTER_NAME", "conveyor tests")
        .env("GIT_COMMITTER_EMAIL", "tests@example.invalid")
        .output()
        .unwrap_or_else(|e| panic!("git {args:?}: {e}"));

    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// A repository with one commit, and the sha of that commit.
struct Origin {
    dir: tempfile::TempDir,
    sha: String,
}

impl Origin {
    fn create() -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        git(dir.path(), &["init", "--quiet", "-b", "master"]);
        std::fs::write(dir.path().join("README.md"), b"origin content\n").expect("write");
        std::fs::write(dir.path().join(".conveyor.toml"), b"# pipeline\n").expect("write");
        git(dir.path(), &["add", "-A"]);
        git(dir.path(), &["commit", "--quiet", "-m", "initial"]);
        let sha = git(dir.path(), &["rev-parse", "HEAD"]);
        Self { dir, sha }
    }

    fn url(&self) -> String {
        format!("file://{}", self.dir.path().display())
    }
}

fn request<'a>(url: &'a str, git_ref: &'a str, sha: &'a str) -> CheckoutRequest<'a> {
    CheckoutRequest {
        clone_url: url,
        git_ref,
        sha,
        timeout: Duration::from_secs(60),
    }
}

#[tokio::test]
async fn a_commit_is_checked_out_at_the_requested_sha() {
    let origin = Origin::create();
    let url = origin.url();
    let work = tempfile::tempdir().expect("temp dir");

    let req = request(&url, "refs/heads/master", &origin.sha);

    let workspace = checkout(work.path(), "run-1", &req)
        .await
        .expect("checkout should succeed");

    assert_eq!(
        std::fs::read_to_string(workspace.root().join("README.md")).expect("read"),
        "origin content\n"
    );
    // The fallback path: a bare file:// repository will not serve a sha
    // directly, so this also proves the full-fetch fallback works.
    assert_eq!(git(workspace.root(), &["rev-parse", "HEAD"]), origin.sha);
}

#[tokio::test]
async fn the_checkout_is_detached_from_any_branch() {
    // A run builds a commit, not a branch. On a branch, a step running
    // `git describe` would report something other than what is being built.
    let origin = Origin::create();
    let url = origin.url();
    let work = tempfile::tempdir().expect("temp dir");

    let req = request(&url, "refs/heads/master", &origin.sha);

    let workspace = checkout(work.path(), "run-1", &req)
        .await
        .expect("checkout");
    let branch = git(workspace.root(), &["rev-parse", "--abbrev-ref", "HEAD"]);
    assert_eq!(branch, "HEAD", "expected a detached HEAD, got {branch}");
}

#[tokio::test]
async fn the_checkout_lands_in_a_directory_named_for_the_run() {
    let origin = Origin::create();
    let url = origin.url();
    let work = tempfile::tempdir().expect("temp dir");

    let req = request(&url, "refs/heads/master", &origin.sha);

    let workspace = checkout(work.path(), "abc-123", &req)
        .await
        .expect("checkout");
    assert_eq!(
        workspace.root().file_name().and_then(|n| n.to_str()),
        Some("run-abc-123")
    );
}

#[tokio::test]
async fn a_retry_does_not_inherit_the_previous_attempts_directory() {
    let origin = Origin::create();
    let url = origin.url();
    let work = tempfile::tempdir().expect("temp dir");

    let req = request(&url, "refs/heads/master", &origin.sha);

    let first = checkout(work.path(), "run-1", &req)
        .await
        .expect("checkout");
    std::fs::write(first.root().join("leftover"), b"from the last attempt").expect("write");

    let second = checkout(work.path(), "run-1", &req)
        .await
        .expect("checkout");
    assert!(
        !second.root().join("leftover").exists(),
        "a retry must start from a clean checkout"
    );
}

#[tokio::test]
async fn removing_a_workspace_deletes_it() {
    let origin = Origin::create();
    let url = origin.url();
    let work = tempfile::tempdir().expect("temp dir");

    let req = request(&url, "refs/heads/master", &origin.sha);

    let workspace = checkout(work.path(), "run-1", &req)
        .await
        .expect("checkout");
    let root = workspace.root().to_path_buf();
    workspace.remove().await.expect("remove");
    assert!(!root.exists());
}

#[tokio::test]
async fn an_unknown_sha_fails_the_checkout() {
    let origin = Origin::create();
    let url = origin.url();
    let work = tempfile::tempdir().expect("temp dir");

    let req = request(
        &url,
        "refs/heads/master",
        "0000000000000000000000000000000000000000",
    );

    let error = checkout(work.path(), "run-1", &req)
        .await
        .expect_err("should not check out");
    assert!(matches!(error, CheckoutError::Git { .. }), "{error:?}");
}

#[tokio::test]
async fn a_hostile_ref_never_reaches_git() {
    let work = tempfile::tempdir().expect("temp dir");
    let req = CheckoutRequest {
        clone_url: "file:///nonexistent",
        git_ref: "--upload-pack=/tmp/pwned",
        sha: "0123456789abcdef0123456789abcdef01234567",
        timeout: Duration::from_secs(5),
    };

    let error = checkout(work.path(), "run-1", &req)
        .await
        .expect_err("should be refused");
    assert!(
        matches!(error, CheckoutError::InvalidRef { .. }),
        "{error:?}"
    );

    // Refused before anything was created, not after.
    assert!(
        !work.path().join("run-run-1").exists(),
        "validation must happen before the workspace is made"
    );
}

#[tokio::test]
async fn a_missing_origin_fails_rather_than_hanging() {
    let work = tempfile::tempdir().expect("temp dir");
    let req = CheckoutRequest {
        clone_url: "file:///definitely/not/a/repository",
        git_ref: "refs/heads/master",
        sha: "0123456789abcdef0123456789abcdef01234567",
        timeout: Duration::from_secs(30),
    };

    let error = checkout(work.path(), "run-1", &req)
        .await
        .expect_err("should not check out");
    assert!(
        matches!(error, CheckoutError::Git { .. }),
        "expected a git failure, got {error:?}"
    );
}
