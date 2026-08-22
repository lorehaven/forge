use pulley::config::Job;
use pulley::rsync::{Change, classify_line, dry_run, run_command, run_command_async, update};
use std::process::Command;

fn job(src: &std::path::Path, dest: &std::path::Path) -> Job {
    Job {
        id: "test-job".to_string(),
        desc: "test sync".to_string(),
        src: src.display().to_string(),
        dest: dest.display().to_string(),
        delete: false,
        skip: vec![],
        no_confirm: true,
        interval: None,
    }
}

#[test]
fn classify_line_recognizes_a_deletion() {
    let change = classify_line("*deleting old-file.txt");
    assert_eq!(change, Some(Change::Delete("old-file.txt".to_string())));
}

#[test]
fn classify_line_recognizes_a_new_file() {
    let change = classify_line(">f+++++++++ new-file.txt");
    assert_eq!(
        change,
        Some(Change::Create(Some("new-file.txt".to_string())))
    );
}

#[test]
fn classify_line_recognizes_a_new_file_received_from_the_other_side() {
    let change = classify_line("<f+++++++++ new-file.txt");
    assert_eq!(
        change,
        Some(Change::Create(Some("new-file.txt".to_string())))
    );
}

#[test]
fn classify_line_recognizes_a_new_directory() {
    let change = classify_line("cd+++++++++ new-dir/");
    assert_eq!(change, Some(Change::Create(Some("new-dir/".to_string()))));
}

#[test]
fn classify_line_recognizes_a_modified_file() {
    let change = classify_line(">f.st...... existing-file.txt");
    assert_eq!(
        change,
        Some(Change::Modify(Some("existing-file.txt".to_string())))
    );
}

#[test]
fn classify_line_counts_a_change_even_without_a_parseable_path() {
    // No space in the line, so there's nothing after the itemize flags to
    // print - but rsync still reported a change, so it must still count.
    let change = classify_line(">f+++++++++");
    assert_eq!(change, Some(Change::Create(None)));
}

#[test]
fn classify_line_ignores_unrelated_output() {
    assert_eq!(classify_line("sending incremental file list"), None);
    assert_eq!(classify_line(""), None);
    assert_eq!(classify_line("total size is 1,234"), None);
}

// `run_command`/`run_command_async` are generic over any `Command`, so
// they can be exercised with harmless short-lived real processes
// instead of `rsync` itself.

#[test]
fn run_command_returns_trimmed_stdout_lines() {
    let mut cmd = Command::new("printf");
    cmd.arg("first\n  second  \nthird/\n");
    let lines = run_command(&mut cmd).unwrap();
    // Lines ending in `/` (directory entries, in rsync's real output)
    // are filtered out.
    assert_eq!(lines, vec!["first".to_string(), "second".to_string()]);
}

#[test]
fn run_command_errors_with_stderr_on_a_nonzero_exit() {
    let mut cmd = Command::new("sh");
    cmd.args(["-c", "echo boom 1>&2; exit 3"]);
    let err = run_command(&mut cmd).unwrap_err();
    assert!(err.to_string().contains("boom"), "{err}");
}

#[test]
fn run_command_async_succeeds_on_a_zero_exit() {
    let mut cmd = Command::new("true");
    run_command_async(&mut cmd).unwrap();
}

#[test]
fn run_command_async_errors_on_a_nonzero_exit() {
    let mut cmd = Command::new("false");
    let err = run_command_async(&mut cmd).unwrap_err();
    assert!(err.to_string().contains("exited with"), "{err}");
}

#[test]
fn dry_run_reports_true_when_new_files_would_be_created() {
    let src = tempfile::tempdir().unwrap();
    let dest = tempfile::tempdir().unwrap();
    std::fs::write(src.path().join("a.txt"), "hello").unwrap();

    let changed = dry_run(&job(src.path(), dest.path())).unwrap();
    assert!(changed);
    // A dry run must never actually write anything.
    assert!(!dest.path().join("a.txt").exists());
}

#[test]
fn dry_run_reports_false_when_source_and_dest_already_match() {
    let src = tempfile::tempdir().unwrap();
    let dest = tempfile::tempdir().unwrap();

    let changed = dry_run(&job(src.path(), dest.path())).unwrap();
    assert!(!changed);
}

#[test]
fn dry_run_reports_deletions_when_dest_has_extra_files_and_delete_is_set() {
    let src = tempfile::tempdir().unwrap();
    let dest = tempfile::tempdir().unwrap();
    std::fs::write(dest.path().join("stale.txt"), "old").unwrap();

    let changed = dry_run(&job(src.path(), dest.path())).unwrap();
    assert!(changed);
    // Dry run only reports; it never actually deletes.
    assert!(dest.path().join("stale.txt").exists());
}

#[test]
fn update_actually_copies_files_from_source_to_dest() {
    let src = tempfile::tempdir().unwrap();
    let dest = tempfile::tempdir().unwrap();
    std::fs::write(src.path().join("a.txt"), "hello").unwrap();

    update(&job(src.path(), dest.path())).unwrap();

    assert_eq!(
        std::fs::read_to_string(dest.path().join("a.txt")).unwrap(),
        "hello"
    );
}

#[test]
fn update_with_delete_removes_files_gone_from_source() {
    let src = tempfile::tempdir().unwrap();
    let dest = tempfile::tempdir().unwrap();
    std::fs::write(dest.path().join("stale.txt"), "old").unwrap();

    let mut config = job(src.path(), dest.path());
    config.delete = true;
    update(&config).unwrap();

    assert!(!dest.path().join("stale.txt").exists());
}

#[test]
fn update_with_skip_excludes_the_named_entry() {
    let src = tempfile::tempdir().unwrap();
    let dest = tempfile::tempdir().unwrap();
    std::fs::write(src.path().join("keep.txt"), "keep").unwrap();
    std::fs::write(src.path().join("skip.txt"), "skip").unwrap();

    let mut config = job(src.path(), dest.path());
    config.skip = vec!["skip.txt".to_string()];
    update(&config).unwrap();

    assert!(dest.path().join("keep.txt").exists());
    assert!(!dest.path().join("skip.txt").exists());
}
