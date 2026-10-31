//! The anvil commands other than `release`, run through the binary in the throwaway workspace of
//! `flow_support`: what each one hands to `cargo` or `docker`, and how it reports a failure.

use crate::flow_support::Workspace;

const DOCKER_CONFIG: &str = "\
[release]
registry = \"test\"
packages = [\"a\", \"b\"]

[docker]
registry = \"reg.example/acme\"

[docker.modules.svc]
packages = [\"a\"]
dockerfile = \"Dockerfile.svc\"
";

/// The calls one anvil command made.
fn calls_of(w: &Workspace, args: &[&str]) -> Vec<String> {
    let before = w.calls().len();
    let out = w.run(args, "");
    assert!(out.ok, "anvil {args:?}: {}", out.text);
    w.calls().split_off(before)
}

#[quench_test::test]
fn build_commands_pass_their_flags_to_cargo() {
    let w = Workspace::new();
    assert_eq!(calls_of(&w, &["build"]), ["build --workspace"]);
    assert_eq!(calls_of(&w, &["build", "--all"]), ["build --workspace"]);
    assert_eq!(
        calls_of(&w, &["build", "--all-features", "--release", "-p", "a"]),
        ["build --all-features --release --package a"]
    );
    assert_eq!(calls_of(&w, &["clean"]), ["clean"]);
    // the package name is cargo's to check
    assert_eq!(
        calls_of(&w, &["build", "-p", "ghost"]),
        ["build --package ghost"]
    );
    assert!(
        !w.run_with(&["build", "--all"], "", &[("ANVIL_TEST_FAIL", "build")])
            .ok
    );
}

#[quench_test::test]
fn lint_and_format_run_clippy_and_rustfmt() {
    let w = Workspace::new();
    let lint = calls_of(&w, &["lint"]);
    assert!(
        lint[0].starts_with("clippy ") && lint[0].ends_with("-- -D warnings"),
        "{lint:?}"
    );
    assert!(
        lint[0].contains("--all-targets") && lint[0].contains("--all-features"),
        "{lint:?}"
    );
    let json = calls_of(&w, &["lint", "--json"]);
    assert!(json[0].contains("--message-format=json "), "{json:?}");
    assert_eq!(calls_of(&w, &["format"]), ["+nightly fmt"]);
    assert_eq!(
        calls_of(&w, &["format", "--check"]),
        ["+nightly fmt --check"]
    );

    // a lint that fails fails the command, and says which step
    let failed = w.run_with(&["lint"], "", &[("ANVIL_TEST_FAIL", "clippy")]);
    assert!(!failed.ok, "{}", failed.text);
}

#[quench_test::test]
fn test_commands_choose_the_runner_and_scope() {
    let w = Workspace::new();
    assert_eq!(calls_of(&w, &["test", "--all"]), ["test --no-fail-fast --workspace"]);
    assert_eq!(
        calls_of(&w, &["test", "-p", "a", "some_test"]),
        ["test --no-fail-fast --package a some_test"]
    );
    assert_eq!(
        calls_of(&w, &["test", "--all", "--ignored"]),
        ["test --no-fail-fast --workspace -- --ignored"]
    );
    assert_eq!(
        calls_of(&w, &["test", "--all", "--list"]),
        ["test --no-fail-fast --workspace -- --list"]
    );
    assert_eq!(
        calls_of(&w, &["test", "--all", "--json"]),
        ["+nightly test --no-fail-fast --workspace -- -Z unstable-options --format json"]
    );
    assert_eq!(
        calls_of(&w, &["nextest", "--all"]),
        ["nextest run --workspace"]
    );
    assert_eq!(
        calls_of(&w, &["nextest", "-p", "a", "--ignored"]),
        ["nextest run --package a --run-ignored ignored-only"]
    );
    assert!(
        !w.run_with(&["test", "--all"], "", &[("ANVIL_TEST_FAIL", "test")])
            .ok
    );
}

#[quench_test::test]
fn the_supply_chain_and_dependency_commands_run_their_tools() {
    let w = Workspace::new();
    assert_eq!(calls_of(&w, &["audit"]), ["audit"]);
    assert_eq!(calls_of(&w, &["audit", "--json"]), ["audit --format json"]);
    assert_eq!(calls_of(&w, &["machete"]), ["machete"]);
    assert_eq!(calls_of(&w, &["machete", "--json"]), ["machete"]);
    assert_eq!(calls_of(&w, &["deny"]), ["deny check"]);
    assert_eq!(
        calls_of(&w, &["deny", "--json"]),
        ["deny --format json check"]
    );
    assert_eq!(calls_of(&w, &["upgrade"]), ["upgrade"]);
    assert_eq!(
        calls_of(&w, &["upgrade", "--incompatible"]),
        ["upgrade --incompatible"]
    );

    let semver = calls_of(&w, &["semver-check", "-p", "a"]);
    assert!(
        semver[0].starts_with("semver-checks --package a --baseline-root "),
        "{semver:?}"
    );
    let with_rev = calls_of(&w, &["semver-check", "-p", "a", "--baseline-rev", "HEAD"]);
    assert_eq!(with_rev, ["semver-checks --package a --baseline-rev HEAD"]);

    assert!(
        !w.run_with(&["audit"], "", &[("ANVIL_TEST_FAIL", "audit")])
            .ok
    );
    assert!(!w.run_with(&["deny"], "", &[("ANVIL_TEST_FAIL", "deny")]).ok);
    assert!(
        !w.run_with(&["machete"], "", &[("ANVIL_TEST_FAIL", "machete")])
            .ok
    );
}

#[quench_test::test]
fn install_builds_and_installs_each_package_from_its_directory() {
    let w = Workspace::new();
    let all = calls_of(&w, &["install", "--all"]);
    assert_eq!(all.len(), 2, "{all:?}");
    assert!(
        all[0].starts_with("install --path ") && all[0].ends_with("crates/a"),
        "{all:?}"
    );
    assert!(all[1].ends_with("crates/b"), "{all:?}");
    let one = calls_of(&w, &["install", "-p", "a"]);
    assert!(one[0].ends_with("crates/a"), "{one:?}");
}

#[quench_test::test]
fn run_builds_then_runs_a_package() {
    let w = Workspace::new();
    assert_eq!(
        calls_of(&w, &["run", "-p", "a"]),
        ["build --all-features --package a", "run --package a"]
    );
}

#[quench_test::test]
fn list_and_config_check_need_no_tools() {
    let w = Workspace::new();
    let names = w.run(&["list"], "");
    assert!(
        names.ok && names.text.contains('a') && names.text.contains('b'),
        "{}",
        names.text
    );
    let json = w.run(&["list", "--format", "json"], "");
    assert!(
        json.ok && json.text.contains("\"packages\""),
        "{}",
        json.text
    );
    assert!(!w.run(&["list", "--format", "bogus"], "").ok);
    assert!(w.run(&["config", "check"], "").ok);
    assert!(w.calls().is_empty());

    // no credentials stored, so there is no registry token to print
    let token = w.run(&["registry-token"], "");
    assert!(
        !token.ok && token.text.contains("no credentials"),
        "{}",
        token.text
    );
}

#[quench_test::test]
fn docker_commands_build_tag_and_push_the_configured_image() {
    let w = Workspace::new();
    w.configure(DOCKER_CONFIG);

    let build = calls_of(&w, &["docker", "build", "-p", "a"]);
    assert_eq!(build.len(), 1);
    assert!(
        build[0].starts_with("docker build -f Dockerfile.svc --network=host"),
        "{build:?}"
    );
    assert!(build[0].contains("--build-arg PROJECT_NAME=a"), "{build:?}");
    assert!(
        build[0].contains("--build-arg RESOURCES_PATH=crates/a"),
        "{build:?}"
    );
    assert!(build[0].ends_with("-t a ."), "{build:?}");

    assert_eq!(
        calls_of(&w, &["docker", "tag", "-p", "a"]),
        ["docker tag a reg.example/acme/svc/a:0.1.0"]
    );
    assert_eq!(
        calls_of(&w, &["docker", "push", "-p", "a"]),
        ["docker push reg.example/acme/svc/a:0.1.0"]
    );

    let release = calls_of(&w, &["docker", "release", "-p", "a"]);
    assert_eq!(release.len(), 3, "{release:?}");
    assert!(
        release[0].starts_with("docker build") && release[1].starts_with("docker tag"),
        "{release:?}"
    );
    assert!(release[2].starts_with("docker push"), "{release:?}");

    assert_eq!(calls_of(&w, &["docker", "build-all"]).len(), 1);
    assert_eq!(calls_of(&w, &["docker", "release-all"]).len(), 3);
}

#[quench_test::test]
fn docker_commands_report_what_cannot_be_done() {
    let w = Workspace::new();
    // no docker section at all
    let unconfigured = w.run(&["docker", "build", "-p", "a"], "");
    assert!(!unconfigured.ok, "{}", unconfigured.text);

    w.configure(DOCKER_CONFIG);
    // a package no module owns
    assert!(!w.run(&["docker", "build", "-p", "b"], "").ok);
    // a daemon that refuses
    let refused = w.run_with(
        &["docker", "push", "-p", "a"],
        "",
        &[("ANVIL_TEST_FAIL", "push")],
    );
    assert!(!refused.ok, "{}", refused.text);
    let release = w.run_with(
        &["docker", "release-all"],
        "",
        &[("ANVIL_TEST_FAIL", "tag")],
    );
    assert!(!release.ok, "{}", release.text);
    assert!(release.text.contains("docker tag a"), "{}", release.text);

    // a registry-less configuration cannot name an image
    w.configure("[docker.modules.svc]\npackages = [\"a\"]\ndockerfile = \"Dockerfile.svc\"\n");
    let none = w.run(&["docker", "tag", "-p", "a"], "");
    assert!(
        !none.ok && none.text.contains("No Docker registry configured"),
        "{}",
        none.text
    );
}

#[quench_test::test]
fn release_with_docker_packages_builds_tags_and_pushes_them() {
    let w = Workspace::new();
    w.configure(DOCKER_CONFIG);
    let out = w.run(&["release", "--all"], "yes\n");
    assert!(out.ok, "{}", out.text);
    let calls = w.calls();
    // `a` is a docker package (built and pushed); `b` is a crate (published)
    assert!(
        calls.iter().any(|c| c.starts_with("docker build")),
        "{calls:?}"
    );
    assert!(
        calls
            .iter()
            .any(|c| c.starts_with("docker push reg.example/acme/svc/a:0.1.0")),
        "{calls:?}"
    );
    assert!(
        calls
            .iter()
            .any(|c| c.starts_with("publish --registry test --package b")),
        "{calls:?}"
    );
    assert!(
        !calls
            .iter()
            .any(|c| c.starts_with("publish --registry test --package a")),
        "{calls:?}"
    );
}
