//! Unit tests for `artifacts.rs`.
//!
//! Uploading needs a warehouse, which these do not have. What they cover is
//! everything conveyor decides *before* the upload: which paths are allowed,
//! which are refused, and that a refusal never takes the job down with it.

use conveyor_service::artifacts::{ArtifactError, WarehouseStore, collect, collect_test_reports};
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
fn workspace(files: &[(&str, &[u8])]) -> (quench_core::tmp::TempDir, Workspace) {
    let dir = quench_core::tmp::TempDir::new().expect("temp dir");
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

#[quench_test::test]
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

#[quench_test::test]
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

#[quench_test::test]
async fn a_symlink_pointing_out_of_the_checkout_is_refused() {
    // Planted in the repository, this is how `..` gets past a textual check.
    let dir = quench_core::tmp::TempDir::new().expect("temp dir");
    let outside = quench_core::tmp::TempDir::new().expect("temp dir");
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

#[quench_test::test]
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

#[quench_test::test]
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

#[quench_test::test]
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

#[quench_test::test]
async fn declaring_nothing_does_nothing() {
    let (_dir, ws) = workspace(&[]);
    let (kept, problems) = collect(None, &ws, "run-1", "job-1", &[]).await;
    assert!(kept.is_empty() && problems.is_empty());
}

#[quench_test::test]
fn no_store_is_built_without_a_warehouse_url() {
    let _guard = env_lock().lock().expect("env lock");
    quench_core::env::remove("WAREHOUSE_URL");
    assert!(
        WarehouseStore::from_env().is_none(),
        "a deployment with no warehouse should keep nothing rather than guess"
    );
}

#[quench_test::test]
fn a_store_is_built_when_a_warehouse_is_configured() {
    let _guard = env_lock().lock().expect("env lock");
    quench_core::env::set(
        "WAREHOUSE_URL",
        "https://warehouse.example.invalid/warehouse",
    );
    assert!(WarehouseStore::from_env().is_some());
    quench_core::env::remove("WAREHOUSE_URL");
}

#[quench_test::test]
fn the_workspace_resolves_a_nested_artifact_path() {
    // Sanity: the paths a real pipeline declares are ordinary relative ones.
    let (dir, ws) = workspace(&[("target/release/thing", b"x")]);
    let resolved = ws.resolve("target/release/thing").expect("resolves");
    assert!(resolved.starts_with(Path::new(dir.path())));
}

#[quench_test::test]
async fn download_reads_back_what_an_upload_addressed() {
    use quench_test::mock::matchers::{header, method, path};
    use quench_test::mock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/files/artifacts/file"))
        .and(header("authorization", "Basic dGVjaDpwdw=="))
        .respond_with(ResponseTemplate::new(200).set_body_string("{\"ok\":true}"))
        .mount(&server)
        .await;
    Mock::given(path("/api/v1/files/artifacts/gone"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    let store = WarehouseStore::new(
        &format!("{}/", server.uri()),
        "artifacts",
        Some(("tech".to_string(), "pw".to_string())),
    );

    let uri = format!(
        "{}/api/v1/files/artifacts/file?path=conveyor%2Frun%2Fsummary.json",
        server.uri()
    );
    assert_eq!(store.download(&uri).await.expect("bytes"), b"{\"ok\":true}");

    let missing = format!("{}/api/v1/files/artifacts/gone", server.uri());
    assert!(matches!(
        store.download(&missing).await,
        Err(ArtifactError::Rejected { status: 404, .. })
    ));

    // An address in some other place is never fetched, whatever the database says.
    assert!(matches!(
        store.download("http://elsewhere.example.invalid/x").await,
        Err(ArtifactError::Foreign { .. })
    ));
}

#[quench_test::test]
async fn test_results_left_in_the_checkout_are_kept_without_being_declared() {
    use quench_test::mock::matchers::{method, path, query_param};
    use quench_test::mock::{Mock, MockServer, ResponseTemplate};

    let (_dir, ws) = workspace(&[
        ("target/debug/quench-test/summary.json", b"{\"summary\":{}}"),
        ("target/debug/quench-test/index.html", b"<html></html>"),
        // Per-binary reports are folded into the summary; only the two merged files are kept.
        ("target/debug/quench-test/api-unit.json", b"{}"),
    ]);

    let server = MockServer::start().await;
    for file in ["summary.json", "index.html"] {
        Mock::given(method("PUT"))
            .and(path("/api/v1/files/artifacts/file"))
            .and(query_param("path", format!("conveyor/run-1/job-1/{file}")))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;
    }
    let store = WarehouseStore::new(&server.uri(), "artifacts", None);

    let (kept, problems) = collect_test_reports(Some(&store), &ws, "run-1", "job-1").await;

    assert!(problems.is_empty(), "{problems:?}");
    let mut names: Vec<_> = kept.iter().map(|c| c.artifact.name.as_str()).collect();
    names.sort_unstable();
    assert_eq!(names, ["index.html", "summary.json"]);
    assert!(
        kept.iter()
            .all(|c| c.artifact.job_id == "job-1" && c.artifact.digest.is_some())
    );
}

#[quench_test::test]
async fn no_test_results_or_no_warehouse_keeps_nothing() {
    use quench_test::mock::MockServer;

    let (_dir, empty) = workspace(&[("src/main.rs", b"fn main() {}")]);
    let server = MockServer::start().await;
    let store = WarehouseStore::new(&server.uri(), "artifacts", None);
    let (kept, problems) = collect_test_reports(Some(&store), &empty, "r", "j").await;
    assert!(kept.is_empty() && problems.is_empty());

    let (_dir, with_results) = workspace(&[("target/debug/quench-test/summary.json", b"{}")]);
    let (kept, problems) = collect_test_reports(None, &with_results, "r", "j").await;
    assert!(kept.is_empty() && problems.is_empty());

    // A warehouse that refuses is reported, not fatal.
    let refusing = MockServer::start().await;
    let store = WarehouseStore::new(&refusing.uri(), "artifacts", None);
    let (kept, problems) = collect_test_reports(Some(&store), &with_results, "r", "j").await;
    assert!(kept.is_empty());
    assert!(matches!(
        problems[0],
        ArtifactError::Rejected { status: 404, .. }
    ));
}

#[quench_test::test]
async fn a_machine_identity_uploads_and_downloads_with_a_bearer_token_fetched_once() {
    use quench_test::mock::matchers::{body_string_contains, header, method, path};
    use quench_test::mock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    // One grant serves every request while the token is good.
    Mock::given(method("POST"))
        .and(path("/gatehouse/api/v1/token"))
        .and(body_string_contains("grant_type=client_credentials"))
        .and(body_string_contains("client_id=conveyor-publish"))
        .and(body_string_contains("client_secret=s3cret"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(quench_serde::json!({"access_token": "tok-1", "expires_in": 900})),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/warehouse/api/v1/files/artifacts/file"))
        .and(header("authorization", "Bearer tok-1"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/warehouse/api/v1/files/artifacts/file"))
        .and(header("authorization", "Bearer tok-1"))
        .respond_with(ResponseTemplate::new(200).set_body_string("stored"))
        .expect(1)
        .mount(&server)
        .await;

    let store = WarehouseStore::with_client(
        &format!("{}/warehouse", server.uri()),
        "artifacts",
        &format!("{}/gatehouse/api/v1/token", server.uri()),
        "conveyor-publish",
        "s3cret",
    );

    let (_dir, ws) = workspace(&[("target/debug/quench-test/summary.json", b"{}")]);
    let (kept, problems) = collect_test_reports(Some(&store), &ws, "run", "job").await;
    assert!(problems.is_empty(), "{problems:?}");
    assert_eq!(kept.len(), 1);
    assert_eq!(
        store.download(&kept[0].artifact.uri).await.expect("bytes"),
        b"stored"
    );
}

#[quench_test::test]
async fn a_refused_grant_is_reported_as_a_rejection() {
    use quench_test::mock::matchers::path;
    use quench_test::mock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    Mock::given(path("/token"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;
    let store = WarehouseStore::with_client(
        &server.uri(),
        "artifacts",
        &format!("{}/token", server.uri()),
        "c",
        "wrong",
    );

    let (_dir, ws) = workspace(&[("target/debug/quench-test/index.html", b"<html/>")]);
    let (kept, problems) = collect_test_reports(Some(&store), &ws, "run", "job").await;
    assert!(kept.is_empty());
    assert!(matches!(
        problems[0],
        ArtifactError::Rejected { status: 401, .. }
    ));
}

#[quench_test::test]
async fn a_security_report_is_kept_under_its_own_names_next_to_the_test_results() {
    use quench_test::mock::matchers::{method, path, query_param};
    use quench_test::mock::{Mock, MockServer, ResponseTemplate};

    let (_dir, ws) = workspace(&[
        ("target/debug/quench-test/summary.json", b"{\"summary\":{}}"),
        (
            "target/security/summary.json",
            b"{\"summary\":{\"total\":1}}",
        ),
        ("target/security/index.html", b"<html>security</html>"),
    ]);

    let server = MockServer::start().await;
    for remote in ["summary.json", "security-summary.json", "security.html"] {
        Mock::given(method("PUT"))
            .and(path("/api/v1/files/artifacts/file"))
            .and(query_param(
                "path",
                format!("conveyor/run-1/job-1/{remote}"),
            ))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;
    }
    let store = WarehouseStore::new(&server.uri(), "artifacts", None);

    let (kept, problems) = collect_test_reports(Some(&store), &ws, "run-1", "job-1").await;

    assert!(problems.is_empty(), "{problems:?}");
    let mut names: Vec<_> = kept.iter().map(|c| c.artifact.name.as_str()).collect();
    names.sort_unstable();
    // The security files must not overwrite (or be mistaken for) the unit tests' `summary.json`.
    assert_eq!(
        names,
        ["security-summary.json", "security.html", "summary.json"]
    );
}

#[quench_test::test]
fn the_runner_is_asked_to_hand_back_the_security_report_too() {
    let exports = conveyor_service::artifacts::exports(&declared(&["dist/app"]));
    assert!(exports.contains(&"dist/app".to_string()));
    assert!(exports.contains(&"target/debug/quench-test/summary.json".to_string()));
    assert!(exports.contains(&"target/security/summary.json".to_string()));
    assert!(exports.contains(&"target/security/index.html".to_string()));
}
