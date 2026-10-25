//! The security gate for forge, one case per stage, so the run's `quench-test` report is the security
//! report conveyor shows. See `quench-security` for the quench equivalent.
//!
//! Needs `--features gate`, cargo-deny and cargo-audit, and the network (advisory database). Run with
//! `--test-threads=1`: the self-test case is a nested `cargo test`.

#![allow(clippy::wildcard_imports)]

use forge_sectest::gate::*;

fn root() -> std::path::PathBuf {
    workspace_root()
}

fn ensure(ran: &Ran, what: &str) {
    assert!(ran.success, "{what} failed:\n{}", ran.tail(40));
}

#[quench_test::test]
fn supply_chain_cargo_deny() {
    ensure(&run(&root(), "cargo", &["deny", "check"], &[]), "cargo deny check");
}

#[quench_test::test]
fn supply_chain_cargo_audit() {
    ensure(
        &run(&root(), "cargo", &["audit", "--deny", "warnings"], &[]),
        "cargo audit (RustSec advisories vs Cargo.lock)",
    );
}

#[quench_test::test]
fn static_unsafe_is_confined_to_allowlisted_files() {
    let root = root();
    let allowed = allowlist(
        &std::fs::read_to_string(root.join("scripts/unsafe-allowlist.txt"))
            .expect("scripts/unsafe-allowlist.txt must exist"),
    );
    // The allowlist uses plain relative paths; the scanner reports `./`-prefixed ones.
    let found: Vec<String> = files_using_unsafe(&root)
        .into_iter()
        .map(|f| f.trim_start_matches("./").to_string())
        .collect();
    let new: Vec<_> = found.iter().filter(|f| !allowed.contains(f)).collect();
    assert!(new.is_empty(), "NEW unsafe, not reviewed (add to scripts/unsafe-allowlist.txt once justified): {new:?}");
    let stale: Vec<_> = allowed.iter().filter(|f| !found.contains(f)).collect();
    assert!(stale.is_empty(), "stale allowlist entries (no longer contain unsafe): {stale:?}");
    assert!(!rust_files(&root).is_empty(), "the scan saw no Rust files under {}", root.display());
}

#[quench_test::test]
fn static_no_secrets_in_sources() {
    let root = root();
    assert!(!text_files(&root).is_empty(), "the scan saw no files under {}", root.display());
    // Reviewed exceptions, one path per line - each a conscious decision that the file is not a secret.
    let allowed = std::fs::read_to_string(root.join("scripts/secrets-allowlist.txt"))
        .map(|text| allowlist(&text))
        .unwrap_or_default();
    let found: Vec<_> = secrets_in(&root)
        .into_iter()
        .filter(|(file, _, _)| !allowed.iter().any(|a| a.trim_start_matches("./") == file.trim_start_matches("./")))
        .collect();
    assert!(found.is_empty(), "secrets in sources (allow a reviewed false positive in scripts/secrets-allowlist.txt): {found:?}");
}

#[quench_test::test]
fn scanner_proves_itself_against_a_vulnerable_and_a_hardened_app() {
    // The live scan below is only worth reading if the scanner is known to detect what it claims to.
    let ran = run(
        &root(),
        "cargo",
        &["test", "-p", "forge-sectest", "--test", "selftest", "--test", "census", "--", "--test-threads=1"],
        &[],
    );
    ensure(&ran, "forge-sectest self-test");
}

quench_test::main!();
