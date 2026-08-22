//! Black-box CLI tests for the network-touching subcommands: each spawns the
//! real `warehouse` binary against a `wiremock` server, the same way
//! `cli_tests.rs` drives the config-only commands.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::tempdir;
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Registers one `local` registry, in the given temp dir, with its
/// docker/crates/files sections all pointing at `server`.
fn add_local_registry(dir: &std::path::Path, server: &MockServer) {
    let mut cmd = Command::cargo_bin("warehouse").unwrap();
    cmd.current_dir(dir);
    cmd.args([
        "docker",
        "registry",
        "add",
        "cli-net-test",
        "--url",
        &server.uri(),
        "--use",
    ])
    .assert()
    .success();

    let mut cmd = Command::cargo_bin("warehouse").unwrap();
    cmd.current_dir(dir);
    cmd.args([
        "crates",
        "registry",
        "add",
        "cli-net-test",
        "--url",
        &server.uri(),
        "--use",
    ])
    .assert()
    .success();

    let mut cmd = Command::cargo_bin("warehouse").unwrap();
    cmd.current_dir(dir);
    cmd.args([
        "files",
        "registry",
        "add",
        "cli-net-test",
        "--url",
        &server.uri(),
        "--use",
    ])
    .assert()
    .success();
}

#[tokio::test]
async fn docker_catalog_and_tags_print_repositories_and_tags() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/_catalog"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "repositories": ["a/b", "c/d"]
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v2/a/b/tags/list"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "name": "a/b",
            "tags": ["latest", "v1"]
        })))
        .mount(&server)
        .await;

    let temp = tempdir().unwrap();
    add_local_registry(temp.path(), &server);

    let mut cmd = Command::cargo_bin("warehouse").unwrap();
    cmd.current_dir(temp.path());
    cmd.args(["docker", "catalog"])
        .assert()
        .success()
        .stdout(predicate::str::contains("a/b"));

    let mut cmd = Command::cargo_bin("warehouse").unwrap();
    cmd.current_dir(temp.path());
    cmd.args(["docker", "tags", "a/b"])
        .assert()
        .success()
        .stdout(predicate::str::contains("latest"));
}

#[tokio::test]
async fn docker_login_saves_credentials() {
    let server = MockServer::start().await;
    let temp = tempdir().unwrap();
    add_local_registry(temp.path(), &server);

    let mut cmd = Command::cargo_bin("warehouse").unwrap();
    cmd.current_dir(temp.path());
    cmd.args([
        "docker",
        "login",
        "--username",
        "user",
        "--password",
        "pass",
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("credentials saved"));

    let saved =
        fs::read_to_string(temp.path().join(".warehouse/registries/cli-net-test.toml")).unwrap();
    assert!(saved.contains("user"));
}

#[tokio::test]
async fn crates_login_search_yank_and_unyank() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/crates"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "crates": [{ "name": "foo", "max_version": "1.2.3", "description": "a crate" }],
            "meta": { "total": 1 }
        })))
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/api/v1/crates/foo/1.2.3/yank"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "ok": true })))
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/api/v1/crates/foo/1.2.3/unyank"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "ok": true })))
        .mount(&server)
        .await;

    let temp = tempdir().unwrap();
    add_local_registry(temp.path(), &server);

    let mut cmd = Command::cargo_bin("warehouse").unwrap();
    cmd.current_dir(temp.path());
    cmd.args(["crates", "login", "--token", "tok"])
        .assert()
        .success()
        .stdout(predicate::str::contains("token saved"));

    let mut cmd = Command::cargo_bin("warehouse").unwrap();
    cmd.current_dir(temp.path());
    cmd.args(["crates", "search", "foo"])
        .assert()
        .success()
        .stdout(predicate::str::contains("foo"));

    let mut cmd = Command::cargo_bin("warehouse").unwrap();
    cmd.current_dir(temp.path());
    cmd.args(["crates", "yank", "foo", "1.2.3"])
        .assert()
        .success()
        .stdout(predicate::str::contains("yanked foo-1.2.3"));

    let mut cmd = Command::cargo_bin("warehouse").unwrap();
    cmd.current_dir(temp.path());
    cmd.args(["crates", "unyank", "foo", "1.2.3"])
        .assert()
        .success()
        .stdout(predicate::str::contains("unyanked foo-1.2.3"));
}

#[tokio::test]
async fn crates_yank_without_a_token_fails_locally() {
    let server = MockServer::start().await;
    let temp = tempdir().unwrap();
    add_local_registry(temp.path(), &server);

    let mut cmd = Command::cargo_bin("warehouse").unwrap();
    cmd.current_dir(temp.path());
    cmd.args(["crates", "yank", "foo", "1.2.3"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no token set"));
}

#[tokio::test]
async fn crates_search_reports_when_nothing_is_found() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/crates"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "crates": [],
            "meta": { "total": 0 }
        })))
        .mount(&server)
        .await;

    let temp = tempdir().unwrap();
    add_local_registry(temp.path(), &server);

    let mut cmd = Command::cargo_bin("warehouse").unwrap();
    cmd.current_dir(temp.path());
    cmd.args(["crates", "search", "nothing"])
        .assert()
        .success()
        .stdout(predicate::str::contains("no crates found"));
}

#[tokio::test]
async fn crates_versions_lists_active_by_default_and_all_with_flag() {
    let server = MockServer::start().await;
    let body = format!(
        "{}\n{}\n",
        serde_json::json!({
            "name": "foo", "vers": "1.0.0", "cksum": "abc", "yanked": false,
            "deps": [], "features": {}
        }),
        serde_json::json!({
            "name": "foo", "vers": "1.1.0", "cksum": "def", "yanked": true,
            "deps": [], "features": {}
        }),
    );
    Mock::given(method("GET"))
        .and(path_regex(r"^/index/"))
        .respond_with(ResponseTemplate::new(200).set_body_string(body))
        .mount(&server)
        .await;

    let temp = tempdir().unwrap();
    add_local_registry(temp.path(), &server);

    let mut cmd = Command::cargo_bin("warehouse").unwrap();
    cmd.current_dir(temp.path());
    cmd.args(["crates", "versions", "foo"])
        .assert()
        .success()
        .stdout(predicate::str::contains("1.0.0").and(predicate::str::contains("1.1.0").not()));

    let mut cmd = Command::cargo_bin("warehouse").unwrap();
    cmd.current_dir(temp.path());
    cmd.args(["crates", "versions", "foo", "--all"])
        .assert()
        .success()
        .stdout(predicate::str::contains("1.0.0").and(predicate::str::contains("1.1.0")));
}

#[tokio::test]
async fn files_storages_ls_upload_preview_and_download() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/files/storages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "storages": [{ "name": "default", "root": "/data" }]
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path_regex(r"^/api/v1/files/default/entries$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "storage": "default",
            "path": "/",
            "entries": [{ "path": "/a.txt", "is_dir": false, "size_bytes": 5 }]
        })))
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path_regex(r"^/api/v1/files/default/file$"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path_regex(r"^/api/v1/files/default/preview$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "storage": "default",
            "path": "/a.txt",
            "kind": "text",
            "content": "hello",
            "truncated": false
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path_regex(r"^/api/v1/files/default/download$"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-disposition", r#"attachment; filename="a.txt""#)
                .set_body_bytes(b"contents".to_vec()),
        )
        .mount(&server)
        .await;

    let temp = tempdir().unwrap();
    add_local_registry(temp.path(), &server);

    let mut cmd = Command::cargo_bin("warehouse").unwrap();
    cmd.current_dir(temp.path());
    cmd.args(["files", "storages"])
        .assert()
        .success()
        .stdout(predicate::str::contains("default"));

    let mut cmd = Command::cargo_bin("warehouse").unwrap();
    cmd.current_dir(temp.path());
    cmd.args(["files", "ls", "default"])
        .assert()
        .success()
        .stdout(predicate::str::contains("a.txt"));

    let local_file = temp.path().join("upload.txt");
    fs::write(&local_file, b"hello").unwrap();
    let mut cmd = Command::cargo_bin("warehouse").unwrap();
    cmd.current_dir(temp.path());
    cmd.args(["files", "upload", "default", local_file.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("uploaded"));

    let mut cmd = Command::cargo_bin("warehouse").unwrap();
    cmd.current_dir(temp.path());
    cmd.args(["files", "preview", "default", "/a.txt"])
        .assert()
        .success()
        .stdout(predicate::str::contains("hello"));

    let mut cmd = Command::cargo_bin("warehouse").unwrap();
    cmd.current_dir(temp.path());
    cmd.args(["files", "download", "default", "/a.txt"])
        .assert()
        .success()
        .stdout(predicate::str::contains("saved a.txt"));
    assert_eq!(
        fs::read(temp.path().join("a.txt")).unwrap(),
        b"contents".to_vec()
    );
}

#[tokio::test]
async fn files_ls_reports_an_empty_directory() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_regex(r"^/api/v1/files/default/entries$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "storage": "default",
            "path": "/",
            "entries": []
        })))
        .mount(&server)
        .await;

    let temp = tempdir().unwrap();
    add_local_registry(temp.path(), &server);

    let mut cmd = Command::cargo_bin("warehouse").unwrap();
    cmd.current_dir(temp.path());
    cmd.args(["files", "ls", "default"])
        .assert()
        .success()
        .stdout(predicate::str::contains("(empty)"));
}

#[tokio::test]
async fn files_mkdir_rmdir_delete_and_bulk_operations() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path_regex(r"^/api/v1/files/default/folder$"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path_regex(r"^/api/v1/files/default/folder$"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path_regex(r"^/api/v1/files/default/file$"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/api/v1/files/default/bulk"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v1/files/default/bulk-download"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"zip-bytes".to_vec()))
        .mount(&server)
        .await;

    let temp = tempdir().unwrap();
    add_local_registry(temp.path(), &server);

    let mut cmd = Command::cargo_bin("warehouse").unwrap();
    cmd.current_dir(temp.path());
    cmd.args(["files", "mkdir", "default", "/new"])
        .assert()
        .success()
        .stdout(predicate::str::contains("folder created"));

    let mut cmd = Command::cargo_bin("warehouse").unwrap();
    cmd.current_dir(temp.path());
    cmd.args(["files", "rmdir", "default", "/new"])
        .assert()
        .success()
        .stdout(predicate::str::contains("folder deleted"));

    let mut cmd = Command::cargo_bin("warehouse").unwrap();
    cmd.current_dir(temp.path());
    cmd.args(["files", "delete", "default", "/a.txt"])
        .assert()
        .success()
        .stdout(predicate::str::contains("file deleted"));

    let mut cmd = Command::cargo_bin("warehouse").unwrap();
    cmd.current_dir(temp.path());
    cmd.args(["files", "bulk-delete", "default", "/a.txt", "/b.txt"])
        .assert()
        .success()
        .stdout(predicate::str::contains("bulk delete complete"));

    let mut cmd = Command::cargo_bin("warehouse").unwrap();
    cmd.current_dir(temp.path());
    cmd.args([
        "files",
        "bulk-download",
        "default",
        "/a.txt",
        "/b.txt",
        "--output",
        "out.zip",
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("saved out.zip"));
    assert_eq!(
        fs::read(temp.path().join("out.zip")).unwrap(),
        b"zip-bytes".to_vec()
    );
}

#[tokio::test]
async fn files_bulk_delete_requires_at_least_one_path() {
    let server = MockServer::start().await;
    let temp = tempdir().unwrap();
    add_local_registry(temp.path(), &server);

    let mut cmd = Command::cargo_bin("warehouse").unwrap();
    cmd.current_dir(temp.path());
    cmd.args(["files", "bulk-delete", "default"])
        .assert()
        .failure();
}

#[tokio::test]
async fn admin_gc_runs_both_docker_and_crates_by_default() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/admin/docker/gc"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "deleted": 1,
            "kept": 2
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/admin/crates/gc"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "deleted_crates": 3,
            "kept_crates": 4,
            "removed_index_entries": 0,
            "deleted_owner_files": 0,
            "removed_empty_dirs": 0
        })))
        .mount(&server)
        .await;

    let temp = tempdir().unwrap();
    add_local_registry(temp.path(), &server);

    let mut cmd = Command::cargo_bin("warehouse").unwrap();
    cmd.current_dir(temp.path());
    cmd.args(["admin", "gc"]).assert().success().stdout(
        predicate::str::contains("Docker GC completed")
            .and(predicate::str::contains("Crates GC completed")),
    );
}

#[tokio::test]
async fn admin_gc_docker_only_skips_crates() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/admin/docker/gc"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "deleted": 0,
            "kept": 0
        })))
        .mount(&server)
        .await;

    let temp = tempdir().unwrap();
    add_local_registry(temp.path(), &server);

    let mut cmd = Command::cargo_bin("warehouse").unwrap();
    cmd.current_dir(temp.path());
    cmd.args(["admin", "gc", "--docker"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Docker GC completed")
                .and(predicate::str::contains("Crates GC completed").not()),
        );
}
