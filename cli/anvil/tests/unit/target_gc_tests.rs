use anvil::target_gc::{dir_size, parse_size, prune};
use std::fs;
use std::path::Path;

const KIB: u64 = 1024;

fn file(path: &Path, kib: u64) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, vec![0u8; usize::try_from(kib * KIB).unwrap()]).unwrap();
}

/// A `target/` with the shape cargo leaves: `debug` 10 KiB deps + 30 KiB incremental, `release` 20 KiB.
fn target() -> quench_core::tmp::TempDir {
    let dir = quench_core::tmp::TempDir::new().unwrap();
    let t = dir.path();
    for profile in ["debug", "release"] {
        file(&t.join(profile).join(".fingerprint/x"), 1);
    }
    file(&t.join("debug/deps/a"), 10);
    file(&t.join("debug/incremental/b"), 30);
    file(&t.join("release/deps/a"), 20);
    file(&t.join("docs/book.html"), 5);
    dir
}

#[quench_test::test]
fn parse_size_reads_units_and_a_bare_number_as_gigabytes() {
    assert_eq!(parse_size("20").unwrap(), 20 << 30);
    assert_eq!(parse_size("20G").unwrap(), 20 << 30);
    assert_eq!(parse_size("512m").unwrap(), 512 << 20);
    assert_eq!(parse_size("1.5GiB").unwrap(), 3 << 29);
    assert_eq!(parse_size("2T").unwrap(), 2 << 40);
    assert!(parse_size("lots").is_err());
    assert!(parse_size("-1G").is_err());
    assert!(parse_size("5X").is_err());
}

#[quench_test::test]
fn under_the_limit_nothing_is_removed() {
    let dir = target();
    let report = prune(dir.path(), 10 << 20, &[], false, false).unwrap();
    assert!(report.removed.is_empty());
    assert!(!report.over);
    assert!(dir.path().join("debug/incremental").exists());
}

#[quench_test::test]
fn incremental_caches_go_before_any_profile() {
    let dir = target();
    let total = dir_size(dir.path());
    let report = prune(dir.path(), total - 10 * KIB, &[], false, false).unwrap();
    assert_eq!(report.removed, [dir.path().join("debug/incremental")]);
    assert!(!report.over);
    assert!(dir.path().join("debug/deps/a").exists());
    assert!(dir.path().join("release/deps/a").exists());
}

#[quench_test::test]
fn then_profiles_go_and_other_directories_stay() {
    let dir = target();
    let report = prune(dir.path(), 2 * KIB, &[], false, false).unwrap();
    assert!(
        report.over,
        "docs/ is never pruned, so 5 KiB+ remain: {report:?}"
    );
    assert!(!dir.path().join("debug").exists());
    assert!(!dir.path().join("release").exists());
    assert!(dir.path().join("docs/book.html").exists());
}

#[quench_test::test]
fn a_kept_profile_is_spared_and_only_loses_its_incremental() {
    let dir = target();
    let report = prune(dir.path(), 0, &["debug"], true, false).unwrap();
    assert!(report.over);
    assert!(dir.path().join("debug/deps/a").exists());
    assert!(!dir.path().join("debug/incremental").exists());
    assert!(!dir.path().join("release").exists());
}

#[quench_test::test]
fn without_sparing_the_kept_profile_goes_last() {
    let dir = target();
    // debug (without its cache) is 11 KiB, release 21 KiB: freeing release alone is enough for a limit of 15 KiB.
    prune(dir.path(), 15 * KIB + 5 * KIB, &["debug"], false, false).unwrap();
    assert!(dir.path().join("debug/deps/a").exists());
    assert!(!dir.path().join("release").exists());
}

#[quench_test::test]
fn a_dry_run_reports_and_removes_nothing() {
    let dir = target();
    let report = prune(dir.path(), 0, &[], false, true).unwrap();
    assert!(!report.removed.is_empty());
    assert!(dir.path().join("debug/incremental/b").exists());
    assert!(dir.path().join("release/deps/a").exists());
}

#[quench_test::test]
fn target_triple_profiles_are_found() {
    let dir = quench_core::tmp::TempDir::new().unwrap();
    file(
        &dir.path()
            .join("wasm32-unknown-unknown/release/.fingerprint/x"),
        1,
    );
    file(
        &dir.path().join("wasm32-unknown-unknown/release/deps/a"),
        10,
    );
    prune(dir.path(), 0, &[], false, false).unwrap();
    assert!(!dir.path().join("wasm32-unknown-unknown/release").exists());
}
