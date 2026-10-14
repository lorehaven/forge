//! `diff --against <git-ref>`: the overlay as it was at a commit, rendered and compared with the
//! overlay as it is, in a real throwaway repository.

use crate::env_support::cwd_lock;
use riveter::against::diff_against;
use riveter::render::{ResourceScope, Selector};
use std::fs;
use std::path::Path;
use std::process::Command;

fn overlay(image: &str) -> String {
    format!(
        "kube_context: c\nenv: prod\nnamespace_name: demo\nresources:\n  - kind: deployment\n    name: api\n    image: {image}\n"
    )
}

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?} failed");
}

/// Puts the cwd back even when the body panics, so one failure does not take the rest with it.
struct Restore(std::path::PathBuf);
impl Drop for Restore {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.0);
    }
}

/// A repository whose overlay is at `v1` with `nginx:1.0`, in `<repo>/<sub>/overlays/prod`, with the
/// cwd set to `<repo>/<sub>` for the body. `sub` may be empty for the repository root.
fn in_repo<T>(sub: &str, body: impl FnOnce(&Path) -> T) -> T {
    let _guard = cwd_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let repo = tempfile::tempdir().unwrap();
    let work = repo.path().join(sub);
    fs::create_dir_all(work.join("overlays/prod")).unwrap();
    fs::write(
        work.join("overlays/prod/overlay.yaml"),
        overlay("nginx:1.0"),
    )
    .unwrap();
    git(repo.path(), &["init", "-q", "."]);
    git(repo.path(), &["add", "."]);
    git(repo.path(), &["commit", "-qm", "one"]);
    git(repo.path(), &["tag", "v1"]);

    let _restore = Restore(std::env::current_dir().unwrap_or_else(|_| std::env::temp_dir()));
    std::env::set_current_dir(&work).unwrap();
    body(&work)
}

fn compare(git_ref: &str) -> anyhow::Result<riveter::against::Comparison> {
    diff_against("prod", git_ref, ResourceScope::All, &Selector::default())
}

#[test]
fn an_unchanged_overlay_renders_the_same_as_its_ref() {
    in_repo("", |_| {
        let found = compare("v1").unwrap();
        assert!(!found.differs);
        assert_eq!(found.git_ref, "v1");
        assert!(found.resource_count > 0);
    });
}

#[test]
fn a_changed_overlay_differs() {
    in_repo("", |work| {
        fs::write(
            work.join("overlays/prod/overlay.yaml"),
            overlay("nginx:2.0"),
        )
        .unwrap();
        assert!(compare("v1").unwrap().differs);
    });
}

#[test]
fn the_ref_is_what_is_compared_not_the_latest_commit() {
    in_repo("", |work| {
        fs::write(
            work.join("overlays/prod/overlay.yaml"),
            overlay("nginx:2.0"),
        )
        .unwrap();
        git(work, &["commit", "-qam", "two"]);
        // The working tree now equals HEAD, but not v1.
        assert!(!compare("HEAD").unwrap().differs);
        assert!(compare("v1").unwrap().differs);
    });
}

#[test]
fn it_works_from_a_subdirectory_of_the_repository() {
    in_repo("deploy/homecloud", |work| {
        assert!(!compare("v1").unwrap().differs);
        fs::write(
            work.join("overlays/prod/overlay.yaml"),
            overlay("nginx:2.0"),
        )
        .unwrap();
        assert!(compare("v1").unwrap().differs);
    });
}

#[test]
fn a_difference_in_source_that_renders_identically_is_not_a_change() {
    in_repo("", |work| {
        // Comments and key order are not in the rendered manifest.
        let reordered = "# a comment\nenv: prod\nnamespace_name: demo\nkube_context: c\nresources:\n  - kind: deployment\n    name: api\n    image: nginx:1.0\n";
        fs::write(work.join("overlays/prod/overlay.yaml"), reordered).unwrap();
        assert!(!compare("v1").unwrap().differs);
    });
}

#[test]
fn the_uncommitted_env_file_stands_in_for_both_sides() {
    in_repo("", |work| {
        // `.env` is not committed, so v1 has none; a variable still resolves on both sides.
        let with_var = |image: &str| overlay(image).replace("nginx", "${REGISTRY}");
        fs::write(
            work.join("overlays/prod/overlay.yaml"),
            with_var("/nginx:1.0"),
        )
        .unwrap();
        git(work, &["add", "-A"]);
        git(work, &["commit", "-qm", "var"]);
        git(work, &["tag", "v2"]);
        fs::write(work.join("overlays/prod/.env"), "REGISTRY=reg.example\n").unwrap();
        assert!(!compare("v2").unwrap().differs);
    });
}

#[test]
fn nothing_is_written_to_the_working_directory() {
    in_repo("", |work| {
        fs::write(
            work.join("overlays/prod/overlay.yaml"),
            overlay("nginx:2.0"),
        )
        .unwrap();
        compare("v1").unwrap();
        assert!(
            !work.join("manifests").exists(),
            "renders go to scratch space"
        );
    });
}

#[test]
fn an_unknown_ref_is_refused_by_name() {
    in_repo("", |_| {
        let err = compare("no-such-ref").unwrap_err().to_string();
        assert!(err.contains("`no-such-ref` is not a commit"), "{err}");
    });
}

#[test]
fn an_environment_that_did_not_exist_at_the_ref_says_so() {
    in_repo("", |work| {
        fs::create_dir_all(work.join("overlays/new")).unwrap();
        fs::write(work.join("overlays/new/overlay.yaml"), overlay("nginx:1.0")).unwrap();
        let err = diff_against("new", "v1", ResourceScope::All, &Selector::default())
            .unwrap_err()
            .to_string();
        assert!(err.contains("`new` has no overlay at v1"), "{err}");
    });
}

#[test]
fn a_blank_ref_is_refused() {
    in_repo("", |_| {
        assert!(compare("  ").is_err());
    });
}
