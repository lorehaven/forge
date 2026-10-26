//! The package commands as `main` runs them (`pack`, `publish`, `pull`, `remote`, `install`):
//! Warehouse is a mock server, the cluster a `kubectl` script that records its calls, and the
//! overlay a directory in a temporary tree.

use crate::env_support::{cwd_lock, env_lock};
use quench_test::mock::matchers::{header, method, path};
use quench_test::mock::{Mock, MockServer, ResponseTemplate};
use riveter::cli::{ApplyScope, RemoteCmd};
use riveter::env::{Workspace, with_workspace};
use riveter::package_cmd::{
    InstallArgs, install_command, pack_command, publish_command, pull_command, remote_command,
};
use riveter::repl::WaitPolicy;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

const OVERLAY: &str = r#"env: demo
namespace_name: demo-ns

resources:
{% include "demo/base.yaml.j2" %}
{% include "demo/app/deployment.yaml.j2" %}
"#;

const DEPLOYMENT: &str = "\
- kind: deployment
  name: api
  image: registry.example:5000/demo/api:1.2.3
  env_vars:
    TOKEN: ${API_TOKEN}
    REGION: ${REGION}
";

fn write(root: &Path, path: &str, text: &str) {
    let target = root.join(path);
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(target, text).unwrap();
}

/// A temp tree holding `overlays/demo`, version 0.4.0.
fn project() -> quench_core::tmp::TempDir {
    let dir = quench_core::tmp::TempDir::new().unwrap();
    let root = dir.path().join("overlays");
    write(&root, "demo/overlay.yaml", OVERLAY);
    write(
        &root,
        "demo/base.yaml.j2",
        "- kind: namespace\n  immutable: true\n",
    );
    write(&root, "demo/app/deployment.yaml.j2", DEPLOYMENT);
    write(
        &root,
        "demo/rivet.toml",
        "[package]\nname = \"demo\"\nversion = \"0.4.0\"\n",
    );
    write(&root, "demo/values.toml", "REGION = \"eu\"\n");
    write(&root, "demo/.env.example", "API_TOKEN=changeme\n");
    dir
}

fn workspace(dir: &quench_core::tmp::TempDir) -> Workspace {
    Workspace {
        overlays_dir: Some(dir.path().join("overlays")),
        ..Workspace::default()
    }
}

/// Packs the project's `demo` overlay unpinned, returning the package file.
fn packed(dir: &quench_core::tmp::TempDir, suffix: Option<&str>) -> PathBuf {
    with_workspace(workspace(dir), || {
        pack_command("demo", suffix, true, &dir.path().join("out"), &[])
    })
    .unwrap()
    .path
}

/// Both process-global locks: the `RIVETER_*` variables (`env_lock`), and the working directory and
/// `PATH` (`cwd_lock`). The order is the one `registry_tests` take them in - the other way round, a test
/// holding each would wait on the other for good.
fn lock_all() -> (
    std::sync::MutexGuard<'static, ()>,
    std::sync::MutexGuard<'static, ()>,
) {
    (
        env_lock()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
        cwd_lock()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
    )
}

fn record(name: &str, version: &str, bytes: &[u8]) -> quench_serde::Value {
    quench_serde::json!({
        "name": name,
        "version": version,
        "description": "demo package",
        "namespace": "demo-ns",
        "size_bytes": bytes.len(),
        "sha256": rivet_package::sha256_hex(bytes),
        "uploaded_by": "ci",
        "yanked": false,
    })
}

fn json(status: u16, body: &quench_serde::Value) -> ResponseTemplate {
    ResponseTemplate::new(status)
        .set_body_raw(quench_serde::json::to_string(body), "application/json")
}

/// Mounts what a registry answers for one published version of `demo`.
async fn serve_version(server: &MockServer, version: &str, bytes: &[u8]) {
    for route in ["latest", version] {
        Mock::given(method("GET"))
            .and(path(format!("/warehouse/api/v1/rivets/demo/{route}")))
            .respond_with(json(200, &record("demo", version, bytes)))
            .mount(server)
            .await;
    }
    Mock::given(method("GET"))
        .and(path(format!(
            "/warehouse/api/v1/rivets/demo/{version}/download"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_raw(bytes.to_vec(), "application/zstd"))
        .mount(server)
        .await;
}

fn point_at(server: &MockServer) {
    quench_core::env::set(
        "RIVETER_WAREHOUSE_URL",
        format!("{}/warehouse", server.uri()),
    );
    quench_core::env::set("RIVETER_WAREHOUSE_TOKEN", "tok");
}

fn forget_warehouse() {
    for var in ["RIVETER_WAREHOUSE_URL", "RIVETER_WAREHOUSE_TOKEN"] {
        quench_core::env::remove(var);
    }
}

/// Runs a blocking call off the async runtime.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    tokio::task::spawn_blocking(f).await.unwrap()
}

async fn puts(server: &MockServer) -> usize {
    server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.method == "PUT")
        .count()
}

// --- pack -------------------------------------------------------------------

#[quench_test::test]
fn pack_writes_the_package_and_expands_the_version_suffix() {
    let _cwd = cwd_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let dir = project();
    let plain = packed(&dir, None);
    assert_eq!(plain, dir.path().join("out/demo-0.4.0.rivet"));
    let suffixed = packed(&dir, Some("build7"));
    assert_eq!(suffixed.file_name().unwrap(), "demo-0.4.0+build7.rivet");

    // a CI commit fills the {sha} token
    quench_core::env::set("CONVEYOR_SHA", "abcdef1234567890");
    let from_ci = packed(&dir, Some("{sha}"));
    quench_core::env::remove("CONVEYOR_SHA");
    assert!(
        from_ci
            .file_name()
            .unwrap()
            .to_string_lossy()
            .contains("abcdef1"),
        "{}",
        from_ci.display()
    );
}

#[quench_test::test]
fn pack_names_an_unknown_environment_and_a_bad_suffix() {
    let _cwd = cwd_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let dir = project();
    let out = dir.path().join("out");
    let missing = with_workspace(workspace(&dir), || {
        pack_command("nope", None, true, &out, &[])
    });
    assert!(missing.is_err());
    let bad = with_workspace(workspace(&dir), || {
        pack_command("demo", Some("{unknown-token}"), true, &out, &[])
    });
    assert!(bad.is_err());
}

// --- publish ----------------------------------------------------------------

#[allow(clippy::await_holding_lock)] // process-global env and cwd; held for the whole test
#[quench_test::test(flavor = "multi_thread", worker_threads = 2)]
async fn publish_packs_the_environment_and_uploads_it() {
    let _locks = lock_all();
    let dir = project();
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path("/warehouse/api/v1/rivets/demo/0.4.0"))
        .and(header("authorization", "Bearer tok"))
        .respond_with(json(201, &record("demo", "0.4.0", b"x")))
        .expect(1)
        .mount(&server)
        .await;
    point_at(&server);

    let work = workspace(&dir);
    let out = dir.path().join("out");
    blocking(move || {
        with_workspace(work, || {
            publish_command(Some("demo"), None, None, false, true, &out, &[])
        })
    })
    .await
    .unwrap();
    forget_warehouse();
    server.verify().await;
}

#[allow(clippy::await_holding_lock)] // process-global env and cwd; held for the whole test
#[quench_test::test(flavor = "multi_thread", worker_threads = 2)]
async fn publish_uploads_a_package_file_and_reports_a_refusal() {
    let _locks = lock_all();
    let dir = project();
    let file = packed(&dir, None);
    let server = MockServer::start().await;
    point_at(&server);
    Mock::given(method("PUT"))
        .respond_with(json(
            409,
            &quench_serde::json!({"error": "this package version has already been published"}),
        ))
        .mount(&server)
        .await;

    let out = dir.path().join("out");
    let err = blocking(move || {
        publish_command(None, Some(&file), None, false, true, &out, &[]).unwrap_err()
    })
    .await;
    forget_warehouse();
    assert!(err.to_string().contains("already published"), "{err}");
}

#[allow(clippy::await_holding_lock)] // process-global env and cwd; held for the whole test
#[quench_test::test(flavor = "multi_thread", worker_threads = 2)]
async fn publish_needs_something_to_publish_and_somewhere_to_publish_it() {
    let _locks = lock_all();
    let dir = project();
    let out = dir.path().join("out");

    // no Warehouse configured
    forget_warehouse();
    let out2 = out.clone();
    let err = blocking(move || {
        publish_command(Some("demo"), None, None, false, true, &out2, &[]).unwrap_err()
    })
    .await;
    assert!(err.to_string().contains("RIVETER_WAREHOUSE_URL"), "{err}");

    // Warehouse but neither an environment nor a file
    let server = MockServer::start().await;
    point_at(&server);
    let err =
        blocking(move || publish_command(None, None, None, false, true, &out, &[]).unwrap_err())
            .await;
    forget_warehouse();
    assert!(err.to_string().contains("name an environment"), "{err}");
}

#[allow(clippy::await_holding_lock)] // process-global env and cwd; held for the whole test
#[quench_test::test(flavor = "multi_thread", worker_threads = 2)]
async fn publish_skips_a_package_that_matches_the_newest_one() {
    let _locks = lock_all();
    let dir = project();
    let file = packed(&dir, None);
    let bytes = fs::read(&file).unwrap();
    let server = MockServer::start().await;
    // the newest published version differs only in its build metadata
    let published = packed(&dir, Some("older"));
    serve_version(&server, "0.4.0+older", &fs::read(&published).unwrap()).await;
    point_at(&server);

    let out = dir.path().join("out");
    let f = file.clone();
    blocking(move || publish_command(None, Some(&f), None, true, true, &out, &[]))
        .await
        .unwrap();
    assert_eq!(puts(&server).await, 0, "unchanged, so nothing uploaded");

    // when the contents differ, it is uploaded
    write(
        &dir.path().join("overlays"),
        "demo/values.toml",
        "REGION = \"us\"\n",
    );
    let changed = packed(&dir, Some("newer"));
    Mock::given(method("PUT"))
        .respond_with(json(201, &record("demo", "0.4.0+newer", &bytes)))
        .mount(&server)
        .await;
    let out = dir.path().join("out");
    blocking(move || publish_command(None, Some(&changed), None, true, true, &out, &[]))
        .await
        .unwrap();
    assert_eq!(puts(&server).await, 1);
    forget_warehouse();
}

#[allow(clippy::await_holding_lock)] // process-global env and cwd; held for the whole test
#[quench_test::test(flavor = "multi_thread", worker_threads = 2)]
async fn publish_uploads_when_the_package_was_never_published() {
    let _locks = lock_all();
    let dir = project();
    let file = packed(&dir, None);
    let server = MockServer::start().await;
    point_at(&server);
    Mock::given(method("GET"))
        .respond_with(json(
            404,
            &quench_serde::json!({"error": "no such package"}),
        ))
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .respond_with(json(201, &record("demo", "0.4.0", b"x")))
        .mount(&server)
        .await;
    let out = dir.path().join("out");
    blocking(move || publish_command(None, Some(&file), None, true, true, &out, &[]))
        .await
        .unwrap();
    forget_warehouse();
    assert_eq!(puts(&server).await, 1);
}

// --- pull and remote ----------------------------------------------------------

#[allow(clippy::await_holding_lock)] // process-global env and cwd; held for the whole test
#[quench_test::test(flavor = "multi_thread", worker_threads = 2)]
async fn pull_downloads_the_newest_or_a_named_version() {
    let _locks = lock_all();
    let dir = project();
    let bytes = fs::read(packed(&dir, None)).unwrap();
    let server = MockServer::start().await;
    serve_version(&server, "0.4.0", &bytes).await;
    point_at(&server);

    let out = dir.path().join("pulled");
    let (o1, o2) = (out.clone(), out.clone());
    blocking(move || pull_command("demo", &o1)).await.unwrap();
    blocking(move || pull_command("demo@0.4.0", &o2))
        .await
        .unwrap();
    assert_eq!(fs::read(out.join("demo-0.4.0.rivet")).unwrap(), bytes);

    // a file is not something to pull, and neither is junk
    let o = out.clone();
    let err = blocking(move || pull_command("demo-0.4.0.rivet", &o).unwrap_err()).await;
    assert!(err.to_string().contains("is a file"), "{err}");
    let err = blocking(move || pull_command("Not A Name", &out).unwrap_err()).await;
    assert!(err.to_string().contains("neither a .rivet file"), "{err}");
    forget_warehouse();
}

#[allow(clippy::await_holding_lock)] // process-global env and cwd; held for the whole test
#[quench_test::test(flavor = "multi_thread", worker_threads = 2)]
async fn pull_refuses_bytes_that_are_not_a_package() {
    let _locks = lock_all();
    let dir = project();
    let server = MockServer::start().await;
    serve_version(&server, "1.0.0", b"this is not an archive").await;
    point_at(&server);
    let out = dir.path().join("pulled");
    let err = blocking(move || pull_command("demo", &out).unwrap_err()).await;
    forget_warehouse();
    assert!(err.to_string().contains("is not a valid package"), "{err}");
}

#[allow(clippy::await_holding_lock)] // process-global env and cwd; held for the whole test
#[quench_test::test(flavor = "multi_thread", worker_threads = 2)]
async fn remote_lists_the_catalog_and_the_versions_of_a_package() {
    let _locks = lock_all();
    let server = MockServer::start().await;
    point_at(&server);
    let mut yanked = record("demo", "0.3.0", b"old");
    yanked["yanked"] = quench_serde::json!(true);
    Mock::given(method("GET"))
        .and(path("/warehouse/api/v1/rivets"))
        .respond_with(json(
            200,
            &quench_serde::json!([record("demo", "0.4.0", b"x")]),
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/warehouse/api/v1/rivets/demo"))
        .respond_with(json(
            200,
            &quench_serde::json!([record("demo", "0.4.0", b"x"), yanked]),
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/warehouse/api/v1/rivets/ghost"))
        .respond_with(json(200, &quench_serde::json!([])))
        .mount(&server)
        .await;

    blocking(|| remote_command(&RemoteCmd::List)).await.unwrap();
    blocking(|| {
        remote_command(&RemoteCmd::Versions {
            name: "demo".into(),
        })
    })
    .await
    .unwrap();
    blocking(|| {
        remote_command(&RemoteCmd::Versions {
            name: "ghost".into(),
        })
    })
    .await
    .unwrap();

    // an empty catalog is reported, not an error
    server.reset().await;
    Mock::given(method("GET"))
        .respond_with(json(200, &quench_serde::json!([])))
        .mount(&server)
        .await;
    blocking(|| remote_command(&RemoteCmd::List)).await.unwrap();

    // a registry that errors is
    server.reset().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;
    assert!(blocking(|| remote_command(&RemoteCmd::List)).await.is_err());
    forget_warehouse();
}

// --- install ------------------------------------------------------------------

struct Cluster {
    log: PathBuf,
    _dir: quench_core::tmp::TempDir,
}

impl Cluster {
    fn calls(&self) -> Vec<String> {
        fs::read_to_string(&self.log)
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }
}

/// Puts a recording `kubectl` first on `PATH`; `namespace` is `present` or `absent`.
fn with_cluster<T>(namespace: &str, body: impl FnOnce(&Cluster) -> T) -> T {
    let dir = quench_core::tmp::TempDir::new().unwrap();
    let log = dir.path().join("calls.log");
    let script = format!(
        "#!/bin/sh
echo \"$@\" >> '{log}'
case \"$1 $2\" in
  \"get namespace\") case '{namespace}' in present) echo namespace/demo-ns ;; esac ;;
  \"config current-context\") echo test ;;
esac
exit 0
",
        log = log.display(),
    );
    let path = dir.path().join("kubectl");
    fs::write(&path, script).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();

    let original = std::env::var("PATH").unwrap_or_default();
    quench_core::env::set("PATH", format!("{}:{original}", dir.path().display()));
    let result = body(&Cluster { log, _dir: dir });
    quench_core::env::set("PATH", &original);
    result
}

const fn install_args<'a>(
    package: &'a str,
    sets: &'a [String],
    replicas: &'a [String],
    except: &'a [String],
) -> InstallArgs<'a> {
    InstallArgs {
        package,
        env_file: None,
        sets,
        replicas,
        except,
        inventory: false,
        dry_run: true,
        wait: WaitPolicy {
            enabled: false,
            timeout_seconds: 5,
        },
        scope: ApplyScope::All,
        targets: &[],
    }
}

#[quench_test::test]
fn install_applies_a_package_file_with_the_values_it_is_given() {
    let _cwd = cwd_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let dir = project();
    let file = packed(&dir, None);
    let package = file.to_string_lossy().into_owned();
    let sets = vec!["API_TOKEN=s3cret".to_string()];

    with_cluster("present", |cluster| {
        // dry run: reaches no namespace
        let mut args = install_args(&package, &sets, &[], &[]);
        args.inventory = true;
        install_command(&args).unwrap();
        let calls = cluster.calls();
        assert!(
            calls
                .iter()
                .any(|c| c.starts_with("apply --dry-run=client")),
            "{calls:?}"
        );
        assert!(
            !calls.iter().any(|c| c.starts_with("get namespace")),
            "a dry run reaches no cluster: {calls:?}"
        );

        // for real: a missing namespace is created first
        let mut args = install_args(&package, &sets, &[], &[]);
        args.dry_run = false;
        install_command(&args).unwrap();
    });

    with_cluster("absent", |cluster| {
        let mut args = install_args(&package, &sets, &[], &[]);
        args.dry_run = false;
        install_command(&args).unwrap();
        let calls = cluster.calls();
        // the namespace is looked for; this package declares its own, so it is applied with the
        // rest - first, as the selection that gates the rest of the manifest
        assert!(
            calls.iter().any(|c| c.starts_with("get namespace demo-ns")),
            "{calls:?}"
        );
        assert!(!calls.iter().any(|c| c.contains("create")), "{calls:?}");
        let applies: Vec<_> = calls.iter().filter(|c| c.starts_with("apply")).collect();
        assert_eq!(applies.len(), 2, "{calls:?}");
        assert!(applies[0].ends_with(".selection.yaml"), "{applies:?}");
        assert!(
            applies.iter().all(|c| !c.contains("--dry-run")),
            "{applies:?}"
        );
    });
}

#[quench_test::test]
fn install_honours_replicas_except_and_reports_what_is_missing() {
    let _cwd = cwd_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let dir = project();
    let package = packed(&dir, None).to_string_lossy().into_owned();
    let sets = vec!["API_TOKEN=x".to_string()];

    with_cluster("present", |_| {
        // everything but the deployment, scaled to zero anyway
        let replicas = vec!["Deployment/api=0".to_string()];
        let except = vec!["deployment/api".to_string()];
        install_command(&install_args(&package, &sets, &replicas, &except)).unwrap();

        // nothing selected
        let except = vec!["deployment/api".to_string(), "namespace".to_string()];
        install_command(&install_args(&package, &sets, &[], &except)).unwrap();

        // the package needs a value that was not given
        let err = install_command(&install_args(&package, &[], &[], &[])).unwrap_err();
        assert!(err.to_string().contains("API_TOKEN"), "{err}");

        // malformed --replicas and --set
        let bad = vec!["no-equals".to_string()];
        assert!(install_command(&install_args(&package, &sets, &bad, &[])).is_err());
        let bad_set = vec!["no-equals".to_string()];
        assert!(install_command(&install_args(&package, &bad_set, &[], &[])).is_err());
    });
}

#[allow(clippy::await_holding_lock)] // process-global env and cwd; held for the whole test
#[quench_test::test(flavor = "multi_thread", worker_threads = 2)]
async fn install_fetches_a_named_package_from_warehouse() {
    let _locks = lock_all();
    let dir = project();
    let bytes = fs::read(packed(&dir, None)).unwrap();
    let server = MockServer::start().await;
    serve_version(&server, "0.4.0", &bytes).await;
    point_at(&server);

    let result = blocking(|| {
        with_cluster("present", |cluster| {
            let sets = vec!["API_TOKEN=x".to_string()];
            let r = install_command(&install_args("demo@0.4.0", &sets, &[], &[]));
            (r, cluster.calls())
        })
    })
    .await;
    forget_warehouse();
    result.0.unwrap();
    assert!(
        result.1.iter().any(|c| c.contains("apply")),
        "{:?}",
        result.1
    );

    // a name that is not on Warehouse fails before any cluster is touched
    let missing = blocking(|| {
        with_cluster("present", |_| {
            install_command(&install_args("ghost", &[], &[], &[]))
        })
    })
    .await;
    assert!(missing.is_err());
}
