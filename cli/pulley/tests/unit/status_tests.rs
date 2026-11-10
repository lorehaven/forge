#![allow(
    clippy::cast_possible_wrap,
    clippy::missing_const_for_fn,
    reason = "test helpers with small numbers"
)]

//! What pulley records of its runs and what `pulley status` makes of it.

use pulley::config::{Config, Job};
use pulley::rsync::{Progress, Stats, parse_progress, parse_stat};
use pulley::state::{KEEP_RUNS, Live, Outcome, Recording, Run, Store, alive, now};
use pulley::status::{bytes, collect, render};
use quench_core::tmp::TempDir;

fn job(id: &str, interval: Option<u64>) -> Job {
    Job {
        id: id.to_string(),
        desc: format!("Backup {id}"),
        src: "/src".to_string(),
        dest: "/dest".to_string(),
        delete: false,
        skip: Vec::new(),
        no_confirm: true,
        interval,
        marker: None,
    }
}

fn config(jobs: Vec<Job>) -> Config {
    Config { jobs }
}

fn synced(started: i64, bytes: u64, sync_secs: u64) -> Run {
    Run {
        started,
        finished: started + 30 + sync_secs as i64,
        scan_secs: 30,
        sync_secs,
        created: 4,
        modified: 1,
        deleted: 0,
        files: 5,
        bytes,
        outcome: Outcome::Synced,
        error: String::new(),
    }
}

// --- reading rsync -----------------------------------------------------------------

#[quench_test::test]
fn a_progress_line_is_read_for_bytes_percent_and_rate() {
    assert_eq!(
        parse_progress("  1048576000  45%   98.00MB/s    0:00:11 (xfr#23, to-chk=0/100)"),
        Some(Progress {
            bytes: 1_048_576_000,
            percent: 45,
            rate: 98 * 1024 * 1024
        })
    );
    assert_eq!(
        parse_progress("1,234  1%  1.50kB/s  0:00:01").map(|p| (p.bytes, p.rate)),
        Some((1234, 1536))
    );
    assert_eq!(parse_progress("Number of files: 12"), None);
    assert_eq!(parse_progress("sending incremental file list"), None);
}

#[quench_test::test]
fn the_final_statistics_give_files_and_bytes() {
    let mut stats = Stats::default();
    for line in [
        "Number of files: 120 (reg: 100, dir: 20)",
        "Number of regular files transferred: 12",
        "Total file size: 99999 bytes",
        "Total transferred file size: 123456789 bytes",
    ] {
        parse_stat(line, &mut stats);
    }
    assert_eq!(
        stats,
        Stats {
            files: 12,
            bytes: 123_456_789
        }
    );

    let mut older = Stats::default();
    parse_stat("Number of files transferred: 3", &mut older);
    assert_eq!(older.files, 3);
}

// --- the store ---------------------------------------------------------------------

#[quench_test::test]
fn runs_are_kept_oldest_first_and_only_the_last_few_are_read() {
    let dir = TempDir::new().unwrap();
    let store = Store::at(dir.path().join("state"));
    for n in 0..5 {
        store.append_run("a", &synced(1000 + n, 10, 1));
    }
    let runs = store.runs("a", 3);
    assert_eq!(
        runs.iter().map(|r| r.started).collect::<Vec<_>>(),
        [1002, 1003, 1004]
    );
    assert_eq!(store.runs("nothing", 3), []);
}

#[quench_test::test]
fn the_history_does_not_grow_without_end() {
    let dir = TempDir::new().unwrap();
    let store = Store::at(dir.path().to_path_buf());
    for n in 0..(KEEP_RUNS * 2 + 5) {
        store.append_run("a", &synced(n as i64, 10, 1));
    }
    let all = store.runs("a", usize::MAX);
    assert!(all.len() <= KEEP_RUNS + 5, "{}", all.len());
    assert_eq!(all.last().unwrap().started, (KEEP_RUNS * 2 + 4) as i64);
}

#[quench_test::test]
fn an_unreadable_line_is_skipped() {
    let dir = TempDir::new().unwrap();
    let store = Store::at(dir.path().to_path_buf());
    store.append_run("a", &synced(1, 10, 1));
    std::fs::write(
        dir.path().join("a.runs.jsonl"),
        format!(
            "not json\n{}\n",
            quench_serde::json::to_string(&synced(2, 10, 1))
        ),
    )
    .unwrap();
    assert_eq!(store.runs("a", 10).len(), 1);
}

#[quench_test::test]
fn a_job_id_cannot_climb_out_of_the_directory() {
    let dir = TempDir::new().unwrap();
    let state = dir.path().join("state");
    let store = Store::at(state.clone());
    store.append_run("../evil", &synced(1, 1, 1));
    assert!(!dir.path().join("evil.runs.jsonl").exists());
    assert!(state.join(".._evil.runs.jsonl").exists());
}

#[quench_test::test]
fn a_recording_is_live_while_it_runs_and_a_run_after() {
    let dir = TempDir::new().unwrap();
    let store = Store::at(dir.path().to_path_buf());
    let mut recording = Recording::begin_in(Some(store.clone()), "lorien");
    let live = store.live("lorien").unwrap();
    assert_eq!(
        (live.phase.as_str(), live.pid),
        ("scanning", std::process::id())
    );

    recording.scanned(10, 2, 1);
    assert_eq!(store.live("lorien").unwrap().phase, "syncing");
    recording.progress(500, 5, 100);
    recording.finish(Outcome::Synced, 12, 4096, "");

    assert_eq!(store.live("lorien"), None);
    let run = store.runs("lorien", 1).remove(0);
    assert_eq!(
        (
            run.created,
            run.modified,
            run.deleted,
            run.files,
            run.bytes,
            run.outcome
        ),
        (10, 2, 1, 12, 4096, Outcome::Synced)
    );
}

#[quench_test::test]
fn a_recording_that_is_dropped_leaves_nothing_running() {
    let dir = TempDir::new().unwrap();
    let store = Store::at(dir.path().to_path_buf());
    {
        let _recording = Recording::begin_in(Some(store.clone()), "a");
        assert!(store.live("a").is_some());
    }
    assert_eq!(store.live("a"), None);
    assert_eq!(store.runs("a", 5), []);
}

#[quench_test::test]
fn a_failed_run_keeps_its_reason() {
    let dir = TempDir::new().unwrap();
    let store = Store::at(dir.path().to_path_buf());
    let recording = Recording::begin_in(Some(store.clone()), "a");
    recording.finish(Outcome::Failed, 0, 0, "rsync exited with 23");
    let run = store.runs("a", 1).remove(0);
    assert_eq!(
        (run.outcome, run.error.as_str()),
        (Outcome::Failed, "rsync exited with 23")
    );
}

#[quench_test::test]
fn this_process_is_alive_and_a_made_up_one_is_not() {
    assert!(alive(std::process::id()));
    if cfg!(target_os = "linux") {
        assert!(!alive(u32::MAX - 1));
    }
}

// --- what status says --------------------------------------------------------------

#[quench_test::test]
fn bytes_are_shown_in_the_unit_that_fits() {
    assert_eq!(bytes(512), "512 B");
    assert_eq!(bytes(2048), "2.0 KB");
    assert_eq!(
        bytes(94 * 1024 * 1024 * 1024 + 200 * 1024 * 1024),
        "94.2 GB"
    );
}

#[quench_test::test]
fn a_running_copy_shows_its_progress_and_speed() {
    let dir = TempDir::new().unwrap();
    let store = Store::at(dir.path().to_path_buf());
    let at = now();
    store.write_live(
        "lorien",
        &Live {
            phase: "syncing".into(),
            pid: std::process::id(),
            started: at - 4200,
            phase_started: at - 4100,
            updated: at,
            bytes: 94 * 1024 * 1024 * 1024,
            percent: 62,
            rate: 18 * 1024 * 1024,
        },
    );
    let statuses = collect(&config(vec![job("lorien", Some(3600))]), &store, &[]);
    assert_eq!(statuses[0].state, "running");
    let text = render(&statuses, at);
    assert!(text.contains("lorien  Backup lorien"), "{text}");
    assert!(
        text.contains("running for 1h10m: copying for 1h08m"),
        "{text}"
    );
    assert!(text.contains("94.0 GB so far"), "{text}");
    assert!(text.contains("23.5 MB/s on average"), "{text}");
    assert!(text.contains("18.0 MB/s right now"), "{text}");
    assert!(
        !text.contains("next run"),
        "a running job has no next run: {text}"
    );
}

#[quench_test::test]
fn a_run_whose_process_is_gone_is_interrupted() {
    let dir = TempDir::new().unwrap();
    let store = Store::at(dir.path().to_path_buf());
    let at = now();
    store.write_live(
        "a",
        &Live {
            phase: "syncing".into(),
            pid: u32::MAX - 1,
            started: at - 600,
            phase_started: at - 500,
            updated: at - 120,
            ..Live::default()
        },
    );
    let statuses = collect(&config(vec![job("a", None)]), &store, &[]);
    if cfg!(target_os = "linux") {
        assert_eq!(statuses[0].state, "interrupted");
        let text = render(&statuses, at);
        assert!(
            text.contains("interrupted: pulley stopped while copying (last seen 2m00s ago)"),
            "{text}"
        );
    }
}

#[quench_test::test]
fn an_idle_job_shows_its_last_run_its_typical_speed_and_when_it_is_due() {
    let dir = TempDir::new().unwrap();
    let store = Store::at(dir.path().to_path_buf());
    let at = now();
    let mib = 1024 * 1024;
    store.append_run("a", &synced(at - 7200, 600 * mib, 10)); // 60 MB/s
    store.append_run("a", &synced(at - 3600, 1200 * mib, 120)); // 10 MB/s
    let statuses = collect(&config(vec![job("a", Some(5400))]), &store, &[]);
    let text = render(&statuses, at);
    assert!(text.contains("last run"), "{text}");
    assert!(
        text.contains("copied 1.2 GB in 2m00s at 10.0 MB/s; scan 30s; 4 new, 1 changed, 0 deleted"),
        "{text}"
    );
    assert!(text.contains("typical copy speed 13.8 MB/s over the last 2 copies (slowest 10.0 MB/s, fastest 60.0 MB/s)"), "{text}");
    assert!(text.contains("next run: in 30m00s"), "{text}");

    let overdue = render(&collect(&config(vec![job("a", Some(60))]), &store, &[]), at);
    assert!(overdue.contains("next run: due now"), "{overdue}");
    let by_hand = render(&collect(&config(vec![job("a", None)]), &store, &[]), at);
    assert!(!by_hand.contains("next run"), "{by_hand}");
}

#[quench_test::test]
fn failures_and_quiet_runs_are_said_plainly() {
    let dir = TempDir::new().unwrap();
    let store = Store::at(dir.path().to_path_buf());
    let at = now();
    store.append_run(
        "bad",
        &Run {
            started: at - 100,
            finished: at - 90,
            outcome: Outcome::Failed,
            error: "rsync exited with 23: vanished".into(),
            ..Run::default()
        },
    );
    store.append_run(
        "quiet",
        &Run {
            started: at - 100,
            finished: at - 40,
            scan_secs: 60,
            outcome: Outcome::NoChanges,
            ..Run::default()
        },
    );
    let text = render(
        &collect(
            &config(vec![job("bad", None), job("quiet", None), job("new", None)]),
            &store,
            &[],
        ),
        at,
    );
    assert!(
        text.contains("FAILED: rsync exited with 23: vanished"),
        "{text}"
    );
    assert!(text.contains("no changes (scan 1m00s)"), "{text}");
    assert!(text.contains("no run recorded yet"), "{text}");
}

#[quench_test::test]
fn only_the_named_jobs_are_shown() {
    let dir = TempDir::new().unwrap();
    let store = Store::at(dir.path().to_path_buf());
    let all = config(vec![job("a", None), job("b", None), job("c", None)]);
    let some = collect(&all, &store, &["c".to_string(), "a".to_string()]);
    assert_eq!(
        some.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
        ["a", "c"]
    );
}

#[quench_test::test]
fn the_json_carries_the_state_and_the_recent_runs() {
    let dir = TempDir::new().unwrap();
    let store = Store::at(dir.path().to_path_buf());
    store.append_run("a", &synced(100, 4096, 2));
    let statuses = collect(&config(vec![job("a", Some(60))]), &store, &[]);
    let json = quench_serde::json::to_string_pretty(&statuses);
    let parsed: quench_serde::Value = quench_serde::json::from_str(&json).unwrap();
    assert_eq!(parsed[0]["id"], "a");
    assert_eq!(parsed[0]["state"], "idle");
    assert_eq!(parsed[0]["runs"][0]["bytes"], 4096);
}
