use pulley::config::Job;
use pulley::rsync::{
    Change, Changes, Output, classify_line, dry_run, is_remote, run_command, run_command_async,
    run_streaming, scan, update, update_quietly,
};
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
        marker: None,
    }
}

#[quench_test::test]
fn classify_line_recognizes_a_deletion() {
    let change = classify_line("*deleting old-file.txt");
    assert_eq!(change, Some(Change::Delete("old-file.txt".to_string())));
}

#[quench_test::test]
fn classify_line_recognizes_a_new_file() {
    let change = classify_line(">f+++++++++ new-file.txt");
    assert_eq!(
        change,
        Some(Change::Create(Some("new-file.txt".to_string())))
    );
}

#[quench_test::test]
fn classify_line_recognizes_a_new_file_received_from_the_other_side() {
    let change = classify_line("<f+++++++++ new-file.txt");
    assert_eq!(
        change,
        Some(Change::Create(Some("new-file.txt".to_string())))
    );
}

#[quench_test::test]
fn classify_line_recognizes_a_new_directory() {
    let change = classify_line("cd+++++++++ new-dir/");
    assert_eq!(change, Some(Change::Create(Some("new-dir/".to_string()))));
}

#[quench_test::test]
fn classify_line_recognizes_a_modified_file() {
    let change = classify_line(">f.st...... existing-file.txt");
    assert_eq!(
        change,
        Some(Change::Modify(Some("existing-file.txt".to_string())))
    );
}

#[quench_test::test]
fn classify_line_counts_a_change_even_without_a_parseable_path() {
    // No space in the line, so there's nothing after the itemize flags to
    // print - but rsync still reported a change, so it must still count.
    let change = classify_line(">f+++++++++");
    assert_eq!(change, Some(Change::Create(None)));
}

#[quench_test::test]
fn classify_line_ignores_unrelated_output() {
    assert_eq!(classify_line("sending incremental file list"), None);
    assert_eq!(classify_line(""), None);
    assert_eq!(classify_line("total size is 1,234"), None);
}

// `run_command`/`run_command_async` are generic over any `Command`, so
// they can be exercised with harmless short-lived real processes
// instead of `rsync` itself.

#[quench_test::test]
fn run_command_returns_trimmed_stdout_lines() {
    let mut cmd = Command::new("printf");
    cmd.arg("first\n  second  \nthird/\n");
    let lines = run_command(&mut cmd).unwrap();
    // Lines ending in `/` (directory entries, in rsync's real output)
    // are filtered out.
    assert_eq!(lines, vec!["first".to_string(), "second".to_string()]);
}

#[quench_test::test]
fn run_command_errors_with_stderr_on_a_nonzero_exit() {
    let mut cmd = Command::new("sh");
    cmd.args(["-c", "echo boom 1>&2; exit 3"]);
    let err = run_command(&mut cmd).unwrap_err();
    assert!(err.to_string().contains("boom"), "{err}");
}

#[quench_test::test]
fn run_command_async_succeeds_on_a_zero_exit() {
    let mut cmd = Command::new("true");
    run_command_async(&mut cmd).unwrap();
}

#[quench_test::test]
fn run_command_async_errors_on_a_nonzero_exit() {
    let mut cmd = Command::new("false");
    let err = run_command_async(&mut cmd).unwrap_err();
    assert!(err.to_string().contains("exited with"), "{err}");
}

#[quench_test::test]
fn dry_run_reports_true_when_new_files_would_be_created() {
    let src = quench_core::tmp::TempDir::new().unwrap();
    let dest = quench_core::tmp::TempDir::new().unwrap();
    std::fs::write(src.path().join("a.txt"), "hello").unwrap();

    let changed = dry_run(&job(src.path(), dest.path())).unwrap();
    assert!(changed);
    // A dry run must never actually write anything.
    assert!(!dest.path().join("a.txt").exists());
}

#[quench_test::test]
fn dry_run_reports_false_when_source_and_dest_already_match() {
    let src = quench_core::tmp::TempDir::new().unwrap();
    let dest = quench_core::tmp::TempDir::new().unwrap();

    let changed = dry_run(&job(src.path(), dest.path())).unwrap();
    assert!(!changed);
}

#[quench_test::test]
fn dry_run_reports_deletions_when_dest_has_extra_files_and_delete_is_set() {
    let src = quench_core::tmp::TempDir::new().unwrap();
    let dest = quench_core::tmp::TempDir::new().unwrap();
    std::fs::write(dest.path().join("stale.txt"), "old").unwrap();

    let changed = dry_run(&job(src.path(), dest.path())).unwrap();
    assert!(changed);
    // Dry run only reports; it never actually deletes.
    assert!(dest.path().join("stale.txt").exists());
}

#[quench_test::test]
fn update_actually_copies_files_from_source_to_dest() {
    let src = quench_core::tmp::TempDir::new().unwrap();
    let dest = quench_core::tmp::TempDir::new().unwrap();
    std::fs::write(src.path().join("a.txt"), "hello").unwrap();

    update(&job(src.path(), dest.path())).unwrap();

    assert_eq!(
        std::fs::read_to_string(dest.path().join("a.txt")).unwrap(),
        "hello"
    );
}

#[quench_test::test]
fn update_with_delete_removes_files_gone_from_source() {
    let src = quench_core::tmp::TempDir::new().unwrap();
    let dest = quench_core::tmp::TempDir::new().unwrap();
    std::fs::write(dest.path().join("stale.txt"), "old").unwrap();

    let mut config = job(src.path(), dest.path());
    config.delete = true;
    update(&config).unwrap();

    assert!(!dest.path().join("stale.txt").exists());
}

#[quench_test::test]
fn update_with_skip_excludes_the_named_entry() {
    let src = quench_core::tmp::TempDir::new().unwrap();
    let dest = quench_core::tmp::TempDir::new().unwrap();
    std::fs::write(src.path().join("keep.txt"), "keep").unwrap();
    std::fs::write(src.path().join("skip.txt"), "skip").unwrap();

    let mut config = job(src.path(), dest.path());
    config.skip = vec!["skip.txt".to_string()];
    update(&config).unwrap();

    assert!(dest.path().join("keep.txt").exists());
    assert!(!dest.path().join("skip.txt").exists());
}

#[quench_test::test]
fn is_remote_recognises_ssh_and_daemon_sources() {
    assert!(is_remote("losseheil@192.168.0.9:/mnt/backup/genealogy/"));
    assert!(is_remote("host:dir"));
    assert!(is_remote("rsync://host/module/"));
}

#[quench_test::test]
fn is_remote_leaves_local_paths_alone() {
    assert!(!is_remote("/mnt/backup/genealogy/"));
    assert!(!is_remote("relative/dir"));
    assert!(!is_remote("./odd:name/dir"));
    assert!(!is_remote("/a/b:c"));
}

// ── The quiet way: counts, not a line per file ───────────────────────────────────────────────────────────────────

#[quench_test::test]
fn changes_are_described_by_kind_and_leave_out_what_is_zero() {
    assert_eq!(Changes::default().describe(), "no changes");
    assert_eq!(Changes::default().total(), 0);
    let some = Changes {
        created: 12,
        modified: 0,
        deleted: 1,
    };
    assert_eq!(some.describe(), "12 new, 1 deleted");
    assert_eq!(some.total(), 13);
    assert_eq!(
        Changes {
            created: 1,
            modified: 2,
            deleted: 3
        }
        .describe(),
        "1 new, 2 changed, 3 deleted"
    );
}

#[quench_test::test]
fn a_scan_counts_new_changed_and_deleted_files_by_kind() {
    let src = quench_core::tmp::TempDir::new().unwrap();
    let dest = quench_core::tmp::TempDir::new().unwrap();
    std::fs::write(src.path().join("same.txt"), "same").unwrap();
    std::fs::write(src.path().join("changed.txt"), "one").unwrap();
    std::fs::write(src.path().join("gone.txt"), "going").unwrap();
    std::fs::create_dir(src.path().join("sub")).unwrap();
    std::fs::write(src.path().join("sub/inside.txt"), "x").unwrap();
    let job = job(src.path(), dest.path());
    update(&job).expect("the first sync");

    std::fs::write(src.path().join("new.txt"), "new").unwrap();
    std::fs::write(src.path().join("sub/another.txt"), "more").unwrap();
    std::fs::write(src.path().join("changed.txt"), "a longer text").unwrap();
    std::fs::remove_file(src.path().join("gone.txt")).unwrap();

    let found = scan(&job, Output::Summary).expect("scan");

    assert_eq!(
        found,
        Changes {
            created: 2,
            modified: 1,
            deleted: 1
        }
    );
    // Nothing was copied or deleted by looking.
    assert!(!dest.path().join("new.txt").exists());
    assert!(dest.path().join("gone.txt").exists());
}

#[quench_test::test]
fn a_scan_of_a_tree_in_sync_finds_nothing_and_shown_or_not_finds_the_same() {
    let src = quench_core::tmp::TempDir::new().unwrap();
    let dest = quench_core::tmp::TempDir::new().unwrap();
    std::fs::write(src.path().join("a.txt"), "a").unwrap();
    let job = job(src.path(), dest.path());

    let before = scan(&job, Output::Summary).unwrap();
    assert_eq!(before, scan(&job, Output::Files).unwrap());
    assert_eq!(before.created, 1);

    update_quietly(&job).expect("sync");
    assert_eq!(scan(&job, Output::Summary).unwrap(), Changes::default());
}

#[quench_test::test]
fn a_quiet_update_copies_and_deletes_like_the_other() {
    let src = quench_core::tmp::TempDir::new().unwrap();
    let dest = quench_core::tmp::TempDir::new().unwrap();
    std::fs::write(src.path().join("a.txt"), "hello").unwrap();
    std::fs::write(dest.path().join("old.txt"), "gone at the source").unwrap();
    let mut job = job(src.path(), dest.path());

    // Without `delete` the old file stays.
    update_quietly(&job).expect("sync");
    assert_eq!(
        std::fs::read_to_string(dest.path().join("a.txt")).unwrap(),
        "hello"
    );
    assert!(dest.path().join("old.txt").exists());

    job.delete = true;
    update_quietly(&job).expect("sync");
    assert!(!dest.path().join("old.txt").exists());
}

#[quench_test::test]
fn a_quiet_update_of_a_source_that_cannot_be_read_fails() {
    let dest = quench_core::tmp::TempDir::new().unwrap();
    let missing = std::env::temp_dir().join(format!("pulley-quiet-missing-{}", std::process::id()));
    assert!(update_quietly(&job(&missing, dest.path())).is_err());
}

#[cfg(unix)]
#[quench_test::test]
fn streaming_hands_over_trimmed_lines_without_the_directory_ones() {
    let mut seen = Vec::new();
    run_streaming(
        Command::new("sh").args(["-c", "printf '  a  \\ndir/\\nb\\n'"]),
        |line| seen.push(line.to_string()),
    )
    .unwrap();
    assert_eq!(seen, ["a", "b"]);
}

#[cfg(unix)]
#[quench_test::test]
fn streaming_keeps_only_the_last_of_the_complaints_for_the_error() {
    let error = run_streaming(
        Command::new("sh").args([
            "-c",
            "for i in $(seq 1 50); do echo err$i >&2; done; exit 3",
        ]),
        |_| {},
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("exited with"), "{error}");
    assert!(error.ends_with("err50"), "{error}");
    assert!(
        error.contains("err31 |") && !error.contains("err30 |"),
        "{error}"
    );
}
