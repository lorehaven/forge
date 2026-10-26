//! The gate's scanners, shown known-bad input. Always on: a gate that cannot tell it is blind is worse
//! than no gate.

#![allow(clippy::wildcard_imports)]

use forge_sectest::gate::*;

fn parts(parts: &[&str]) -> String {
    parts.concat()
}

#[quench_test::test]
fn unsafe_in_every_spelling_is_noticed_and_comments_are_not() {
    let kw = parts(&["uns", "afe"]);
    let lint = parts(&["uns", "afe_code"]);
    for bad in [
        format!("let x = {kw} {{ f() }};"),
        format!("{kw}{{ f() }}"),
        format!("pub {kw} fn f() {{}}"),
        format!("{kw} impl Send for X {{}}"),
        format!("#![allow({lint})]"),
        format!("#[allow({lint})]"),
        format!("#![forbid({lint})]"),
        format!("#[{kw}(no_mangle)]"),
    ] {
        assert!(line_uses_unsafe(&bad), "missed: {bad}");
    }
    for fine in [
        format!("// {kw} {{ is discussed in a comment }}"),
        "let safe = 1;".to_string(),
        format!("let unsafely_named = 2; // not the keyword: {}", "uns"),
    ] {
        assert!(!line_uses_unsafe(&fine), "false alarm: {fine}");
    }
}

#[quench_test::test]
fn secrets_are_caught_by_shape_not_by_a_lucky_substring() {
    let begin = parts(&["-----BE", "GIN RSA PRIV", "ATE KEY-----"]);
    assert_eq!(line_leaks_secret(&begin), Some("a private key"));
    assert_eq!(
        line_leaks_secret(&format!(
            "key = \"{}ABCDEFGHIJKLMNOP\"",
            "AK".to_string() + "IA"
        )),
        Some("an AWS access key id")
    );
    assert_eq!(
        line_leaks_secret(&format!(
            "t = {}{}",
            "gh".to_string() + "p_",
            "a".repeat(36)
        )),
        Some("a GitHub personal access token")
    );
    assert_eq!(
        line_leaks_secret(&format!("{}{}", "xo".to_string() + "xb-", "1234567890-abc")),
        Some("a Slack token")
    );
    // Too short to be the real thing, or merely mentioning the word.
    assert_eq!(
        line_leaks_secret(&format!("{}SHORT", "AK".to_string() + "IA")),
        None
    );
    assert_eq!(
        line_leaks_secret("the private key is stored elsewhere"),
        None
    );
    assert_eq!(line_leaks_secret("-----BEGIN CERTIFICATE-----"), None);
}

#[quench_test::test]
fn the_filesystem_scan_finds_a_planted_violation_and_skips_build_output() {
    let dir = quench_core::tmp::TempDir::new().unwrap();
    let root = dir.path();
    let kw = parts(&["uns", "afe"]);
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("target/debug")).unwrap();
    std::fs::write(
        root.join("src/bad.rs"),
        format!("fn f() {{ {kw} {{ g() }} }}\n"),
    )
    .unwrap();
    std::fs::write(root.join("src/good.rs"), "fn f() {}\n").unwrap();
    std::fs::write(
        root.join("target/debug/ignored.rs"),
        format!("{kw} fn x() {{}}\n"),
    )
    .unwrap();
    std::fs::write(
        root.join("src/leak.toml"),
        format!(
            "k = \"{}\"\n",
            parts(&["-----BE", "GIN PRIV", "ATE KEY-----"])
        ),
    )
    .unwrap();

    assert_eq!(files_using_unsafe(root), vec!["./src/bad.rs".to_string()]);
    let secrets = secrets_in(root);
    assert_eq!(secrets.len(), 1, "{secrets:?}");
    assert_eq!(secrets[0].0, "./src/leak.toml");
}

#[quench_test::test]
fn allowlists_ignore_comments_and_blanks() {
    assert_eq!(
        allowlist("# why\n\n./a.rs\n  ./b.rs  \n#./c.rs\n"),
        ["./a.rs", "./b.rs"]
    );
}

#[quench_test::test]
fn a_missing_program_is_a_failed_run_with_a_reason_not_a_pass() {
    let ran = run(
        std::path::Path::new("."),
        "definitely-not-a-real-program-xyz",
        &[],
        &[],
    );
    assert!(!ran.success);
    assert!(ran.output.contains("is it installed"), "{}", ran.output);
}

#[quench_test::test]
fn inside_a_git_checkout_only_project_files_are_scanned() {
    let dir = quench_core::tmp::TempDir::new().unwrap();
    let root = dir.path();
    let leak = format!(
        "k = \"{}\"\n",
        parts(&["-----BE", "GIN PRIV", "ATE KEY-----"])
    );
    let git = |args: &[&str]| assert!(run(root, "git", args, &[]).success, "git {args:?}");
    git(&["init", "--quiet"]);
    std::fs::write(root.join(".gitignore"), "local.toml\nvenv/\n").unwrap();
    std::fs::create_dir_all(root.join("venv")).unwrap();
    std::fs::write(root.join("local.toml"), &leak).unwrap(); // ignored: a developer's own file
    std::fs::write(root.join("venv/lib.toml"), &leak).unwrap(); // ignored: not source
    std::fs::write(root.join("tracked.toml"), &leak).unwrap(); // committed: a real leak
    git(&["add", "tracked.toml", ".gitignore"]);

    let secrets = secrets_in(root);
    assert_eq!(secrets.len(), 1, "{secrets:?}");
    assert_eq!(secrets[0].0, "./tracked.toml");
}

#[cfg(unix)]
#[quench_test::test]
fn symlinks_that_leave_the_project_are_found_and_are_not_followed_by_the_secret_scan() {
    use std::os::unix::fs::symlink;
    let outside = quench_core::tmp::TempDir::new().unwrap();
    let leak = format!(
        "k = \"{}\"\n",
        parts(&["-----BE", "GIN PRIV", "ATE KEY-----"])
    );
    std::fs::write(outside.path().join("local.pem"), &leak).unwrap();

    let dir = quench_core::tmp::TempDir::new().unwrap();
    let root = dir.path();
    let git = |args: &[&str]| assert!(run(root, "git", args, &[]).success, "git {args:?}");
    git(&["init", "--quiet"]);
    std::fs::create_dir_all(root.join("a/b")).unwrap();
    std::fs::write(root.join("a/real.toml"), "x = 1\n").unwrap();
    // Fine: relative, and stays inside (also through `..`).
    symlink("../real.toml", root.join("a/b/inside.toml")).unwrap();
    symlink("real.toml", root.join("a/sibling.toml")).unwrap();
    // Not fine: absolute, and one that climbs out of the project.
    symlink(
        outside.path().join("local.pem"),
        root.join("a/absolute.pem"),
    )
    .unwrap();
    symlink("../../../elsewhere/key.pem", root.join("a/b/climbs.pem")).unwrap();
    git(&["add", "-A"]);

    let found = symlinks_leaving_the_project(root);
    let paths: Vec<&str> = found.iter().map(|(path, _)| path.as_str()).collect();
    assert_eq!(paths, ["./a/absolute.pem", "./a/b/climbs.pem"], "{found:?}");

    // A symlink to a key kept outside the project is not a key in the project.
    assert!(secrets_in(root).is_empty(), "{:?}", secrets_in(root));
}

quench_test::main!();
