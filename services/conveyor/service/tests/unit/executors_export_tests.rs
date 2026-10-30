use conveyor_service::executors::export::Exports;

fn encode(bytes: &[u8]) -> String {
    quench_core::base64::STANDARD.encode(bytes)
}

#[quench_test::test]
fn chunks_are_put_back_together_and_written_under_the_root() {
    let dir = quench_core::tmp::TempDir::new().expect("temp dir");
    let whole = encode(b"hello, report");
    let (first, second) = whole.split_at(8);

    let mut exports = Exports::new(&["target/debug/quench-test/summary.json".to_string()]);
    assert!(!exports.is_empty());
    exports.accept(&format!("target/debug/quench-test/summary.json\t{first}"));
    exports.accept(&format!(
        "target/debug/quench-test/summary.json\t{second}\n"
    ));

    assert!(exports.write(dir.path()).is_empty());
    assert_eq!(
        std::fs::read(dir.path().join("target/debug/quench-test/summary.json")).expect("file"),
        b"hello, report"
    );
}

#[quench_test::test]
fn a_path_nobody_asked_for_is_never_written() {
    let dir = quench_core::tmp::TempDir::new().expect("temp dir");
    let mut exports = Exports::new(&["wanted.txt".to_string()]);
    exports.accept(&format!("unwanted.txt\t{}", encode(b"x")));
    exports.accept("no tab at all");
    exports.accept(&format!("../escape.txt\t{}", encode(b"x")));

    assert!(exports.write(dir.path()).is_empty());
    assert!(std::fs::read_dir(dir.path()).expect("dir").next().is_none());
    assert!(Exports::new(&[]).is_empty());
}

#[quench_test::test]
fn a_requested_path_that_leaves_the_checkout_is_refused() {
    let dir = quench_core::tmp::TempDir::new().expect("temp dir");
    let mut exports = Exports::new(&["../outside.txt".to_string(), "/etc/absolute".to_string()]);
    exports.accept(&format!("../outside.txt\t{}", encode(b"x")));
    exports.accept(&format!("/etc/absolute\t{}", encode(b"x")));

    let problems = exports.write(dir.path());
    assert_eq!(problems.len(), 2, "{problems:?}");
    assert!(
        problems
            .iter()
            .all(|p| p.contains("not inside the checkout"))
    );
}

#[quench_test::test]
fn damaged_and_unwritable_files_are_reported_not_fatal() {
    let dir = quench_core::tmp::TempDir::new().expect("temp dir");
    std::fs::write(
        dir.path().join("blocker"),
        b"a file where a directory is needed",
    )
    .expect("write");

    let mut exports = Exports::new(&["bad.txt".to_string(), "blocker/inner.txt".to_string()]);
    exports.accept("bad.txt\t!!not base64!!");
    exports.accept(&format!("blocker/inner.txt\t{}", encode(b"x")));

    let problems = exports.write(dir.path());
    assert_eq!(problems.len(), 2, "{problems:?}");
    assert!(problems.iter().any(|p| p.contains("damaged")));
    assert!(problems.iter().any(|p| p.contains("could not write")));
}
