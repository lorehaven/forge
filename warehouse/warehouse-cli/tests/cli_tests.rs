use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_registry_management() {
    let temp = tempdir().unwrap();
    let config_dir = temp.path().join(".warehouse");
    fs::create_dir_all(&config_dir).unwrap();

    let mut cmd = Command::cargo_bin("warehouse").unwrap();
    cmd.current_dir(temp.path());

    // 1. Add docker registry
    cmd.args([
        "docker",
        "registry",
        "add",
        "local",
        "--url",
        "http://localhost:8080",
        "--use",
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("registry 'local' saved"));

    // 2. List registries
    let mut cmd = Command::cargo_bin("warehouse").unwrap();
    cmd.current_dir(temp.path());
    cmd.args(["docker", "registry", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "* local -> http://localhost:8080/v2",
        ));

    // 3. Add crates registry
    let mut cmd = Command::cargo_bin("warehouse").unwrap();
    cmd.current_dir(temp.path());
    cmd.args([
        "crates",
        "registry",
        "add",
        "my-crates",
        "--url",
        "http://localhost:8081",
        "--use",
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains(
        "crates registry 'my-crates' saved",
    ));

    // 4. List crates registries
    let mut cmd = Command::cargo_bin("warehouse").unwrap();
    cmd.current_dir(temp.path());
    cmd.args(["crates", "registry", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "* my-crates -> http://localhost:8081",
        ));

    // 5. Add files registry
    let mut cmd = Command::cargo_bin("warehouse").unwrap();
    cmd.current_dir(temp.path());
    cmd.args([
        "files",
        "registry",
        "add",
        "my-files",
        "--url",
        "http://localhost:8082",
        "--use",
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("files registry 'my-files' saved"));

    // 6. List files registries
    let mut cmd = Command::cargo_bin("warehouse").unwrap();
    cmd.current_dir(temp.path());
    cmd.args(["files", "registry", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "* my-files -> http://localhost:8082",
        ));

    // 7. Use another registry (docker)
    let mut cmd = Command::cargo_bin("warehouse").unwrap();
    cmd.current_dir(temp.path());
    cmd.args([
        "docker",
        "registry",
        "add",
        "remote",
        "--url",
        "http://remote:8080",
    ])
    .assert()
    .success();

    let mut cmd = Command::cargo_bin("warehouse").unwrap();
    cmd.current_dir(temp.path());
    cmd.args(["docker", "registry", "use", "remote"])
        .assert()
        .success()
        .stdout(predicate::str::contains("active registry set to 'remote'"));

    // 8. Remove registry
    let mut cmd = Command::cargo_bin("warehouse").unwrap();
    cmd.current_dir(temp.path());
    cmd.args(["docker", "registry", "remove", "local"])
        .assert()
        .success()
        .stdout(predicate::str::contains("registry 'local' removed"));
}

#[test]
fn test_global_local_config() {
    let temp = tempdir().unwrap();
    let home_dir = temp.path().join("home");
    let project_dir = temp.path().join("project");
    fs::create_dir_all(&home_dir).unwrap();
    fs::create_dir_all(&project_dir).unwrap();

    // Set HOME to mock global config location
    let mut cmd = Command::cargo_bin("warehouse").unwrap();
    cmd.current_dir(&project_dir).env("HOME", &home_dir);

    // 1. Add global registry
    cmd.args([
        "docker",
        "registry",
        "add",
        "global-reg",
        "--url",
        "http://global",
        "--global",
    ])
    .assert()
    .success();

    // 2. Add local registry
    let mut cmd = Command::cargo_bin("warehouse").unwrap();
    cmd.current_dir(&project_dir).env("HOME", &home_dir);
    cmd.args([
        "docker",
        "registry",
        "add",
        "local-reg",
        "--url",
        "http://local",
    ])
    .assert()
    .success();

    // 3. List should show both
    let mut cmd = Command::cargo_bin("warehouse").unwrap();
    cmd.current_dir(&project_dir).env("HOME", &home_dir);
    cmd.args(["docker", "registry", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "global-reg -> http://global/v2 (global)",
        ))
        .stdout(predicate::str::contains(
            "local-reg -> http://local/v2 (local)",
        ));

    // 4. Local should override current registry
    let mut cmd = Command::cargo_bin("warehouse").unwrap();
    cmd.current_dir(&project_dir).env("HOME", &home_dir);
    cmd.args(["docker", "registry", "use", "global-reg", "--global"])
        .assert()
        .success();

    let mut cmd = Command::cargo_bin("warehouse").unwrap();
    cmd.current_dir(&project_dir).env("HOME", &home_dir);
    cmd.args(["docker", "registry", "use", "local-reg"])
        .assert()
        .success();

    let mut cmd = Command::cargo_bin("warehouse").unwrap();
    cmd.current_dir(&project_dir).env("HOME", &home_dir);
    cmd.args(["docker", "registry", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("* local-reg"));
}

#[test]
fn test_registry_validation() {
    let temp = tempdir().unwrap();
    let mut cmd = Command::cargo_bin("warehouse").unwrap();
    cmd.current_dir(temp.path());

    // Invalid name
    cmd.args([
        "docker",
        "registry",
        "add",
        "invalid/name",
        "--url",
        "http://localhost",
    ])
    .assert()
    .failure()
    .stderr(predicate::str::contains(
        "registry name cannot contain path separators",
    ));
}

#[test]
fn test_help_output() {
    let mut cmd = Command::cargo_bin("warehouse").unwrap();
    cmd.arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Warehouse CLI"))
        .stdout(predicate::str::contains("docker"))
        .stdout(predicate::str::contains("crates"))
        .stdout(predicate::str::contains("files"))
        .stdout(predicate::str::contains("admin"));
}
