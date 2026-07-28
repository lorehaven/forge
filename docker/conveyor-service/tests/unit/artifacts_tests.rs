//! Unit tests for `artifacts.rs`.
//!
//! Uploading needs a warehouse, which these do not have. What they cover is
//! everything conveyor decides *before* the upload: which paths are allowed,
//! which are refused, and that a refusal never takes the job down with it.

use conveyor_service::artifacts::{ArtifactError, WarehouseStore, collect};
use conveyor_service::workspace::Workspace;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

/// `WarehouseStore::from_env` reads the process environment, which the whole
/// binary shares. The two tests that set and clear `WAREHOUSE_URL` take turns
/// rather than racing over it.
fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// A checkout with the given files in it.
fn workspace(files: &[(&str, &[u8])]) -> (tempfile::TempDir, Workspace) {
    let dir = tempfile::tempdir().expect("temp dir");
    for (path, contents) in files {
        let full = dir.path().join(path);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent).expect("mkdir");
        }
        std::fs::write(&full, contents).expect("write");
    }
    let workspace = Workspace::new(dir.path().to_path_buf());
    (dir, workspace)
}

fn declared(paths: &[&str]) -> Vec<String> {
    paths.iter().map(|p| (*p).to_string()).collect()
}

#[tokio::test]
async fn with_no_store_nothing_is_kept_and_nothing_is_wrong() {
    // A deployment with no warehouse still builds; it just keeps nothing.
    let (_dir, ws) = workspace(&[("target/release/thing", b"binary")]);

    let (kept, problems) = collect(
        None,
        &ws,
        "run-1",
        "job-1",
        &declared(&["target/release/thing"]),
    )
    .await;

    assert!(kept.is_empty());
    assert!(problems.is_empty(), "{problems:?}");
}

#[tokio::test]
async fn a_path_that_climbs_out_of_the_checkout_is_refused() {
    // A pipeline can say `artifacts = ["../../etc/passwd"]`, and collecting it
    // would hand a repository author whatever the service account can read.
    let (_dir, ws) = workspace(&[]);

    let (_, problems) = collect(
        None,
        &ws,
        "run-1",
        "job-1",
        &declared(&["../../etc/passwd", "/etc/passwd"]),
    )
    .await;

    assert_eq!(problems.len(), 2, "{problems:?}");
    assert!(
        problems
            .iter()
            .all(|p| matches!(p, ArtifactError::Outside { .. })),
        "{problems:?}"
    );
}

#[tokio::test]
async fn a_symlink_pointing_out_of_the_checkout_is_refused() {
    // Planted in the repository, this is how `..` gets past a textual check.
    let dir = tempfile::tempdir().expect("temp dir");
    let outside = tempfile::tempdir().expect("temp dir");
    std::fs::write(outside.path().join("secret"), b"x").expect("write");

    #[cfg(unix)]
    std::os::unix::fs::symlink(outside.path(), dir.path().join("escape")).expect("symlink");

    let ws = Workspace::new(dir.path().to_path_buf());
    let (_, problems) = collect(None, &ws, "run-1", "job-1", &declared(&["escape/secret"])).await;

    assert!(
        matches!(problems.first(), Some(ArtifactError::Outside { .. })),
        "{problems:?}"
    );
}

#[tokio::test]
async fn a_declared_artifact_the_job_did_not_produce_is_reported() {
    let (_dir, ws) = workspace(&[]);
    let (_, problems) = collect(
        None,
        &ws,
        "run-1",
        "job-1",
        &declared(&["target/release/never-built"]),
    )
    .await;

    assert!(
        matches!(problems.first(), Some(ArtifactError::Missing { .. })),
        "{problems:?}"
    );
    assert!(
        problems[0].to_string().contains("never-built"),
        "the message should name the path: {}",
        problems[0]
    );
}

#[tokio::test]
async fn a_directory_is_not_an_artifact() {
    // Collecting one would mean deciding how to pack it, which the pipeline
    // can do for itself with a `run` step and a tarball.
    let (dir, ws) = workspace(&[]);
    std::fs::create_dir_all(dir.path().join("target/release")).expect("mkdir");

    let (_, problems) = collect(None, &ws, "run-1", "job-1", &declared(&["target/release"])).await;
    assert!(
        matches!(problems.first(), Some(ArtifactError::Missing { .. })),
        "{problems:?}"
    );
}

#[tokio::test]
async fn one_bad_path_does_not_stop_the_others_being_considered() {
    let (_dir, ws) = workspace(&[("good", b"contents")]);

    let (_, problems) = collect(
        None,
        &ws,
        "run-1",
        "job-1",
        &declared(&["../escape", "missing", "good"]),
    )
    .await;

    // The escape and the missing file are both reported; `good` is fine and
    // simply is not kept, because there is no store.
    assert_eq!(problems.len(), 2, "{problems:?}");
}

#[tokio::test]
async fn declaring_nothing_does_nothing() {
    let (_dir, ws) = workspace(&[]);
    let (kept, problems) = collect(None, &ws, "run-1", "job-1", &[]).await;
    assert!(kept.is_empty() && problems.is_empty());
}

#[test]
fn no_store_is_built_without_a_warehouse_url() {
    let _guard = env_lock().lock().expect("env lock");
    unsafe { std::env::remove_var("WAREHOUSE_URL") };
    assert!(
        WarehouseStore::from_env().is_none(),
        "a deployment with no warehouse should keep nothing rather than guess"
    );
}

#[test]
fn a_store_is_built_when_a_warehouse_is_configured() {
    let _guard = env_lock().lock().expect("env lock");
    unsafe {
        std::env::set_var(
            "WAREHOUSE_URL",
            "https://warehouse.example.invalid/warehouse",
        );
    }
    assert!(WarehouseStore::from_env().is_some());
    unsafe { std::env::remove_var("WAREHOUSE_URL") };
}

#[test]
fn the_workspace_resolves_a_nested_artifact_path() {
    // Sanity: the paths a real pipeline declares are ordinary relative ones.
    let (dir, ws) = workspace(&[("target/release/thing", b"x")]);
    let resolved = ws.resolve("target/release/thing").expect("resolves");
    assert!(resolved.starts_with(Path::new(dir.path())));
}
