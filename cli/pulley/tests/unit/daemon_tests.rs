use pulley::config::{Config, Job};
use pulley::daemon::{is_due, poll_once, run, run_job};
use std::collections::HashMap;
use std::time::{Duration, Instant};

#[test]
fn run_returns_immediately_when_no_job_has_an_interval() {
    // With no `interval` set on any job, `run` takes its early-return branch
    // before ever entering the (otherwise infinite) polling loop.
    let config = Config {
        jobs: vec![Job {
            id: "no-interval".to_string(),
            desc: "desc".to_string(),
            src: "/src".to_string(),
            dest: "/dest".to_string(),
            delete: false,
            skip: Vec::new(),
            no_confirm: true,
            interval: None,
        }],
    };
    run(&config).expect("run");
}

#[test]
fn run_returns_immediately_for_an_empty_job_list() {
    let config = Config { jobs: Vec::new() };
    run(&config).expect("run");
}

#[test]
fn is_due_when_never_run_before() {
    assert!(is_due(None, Instant::now(), Duration::from_secs(10)));
}

#[test]
fn is_not_due_immediately_after_running() {
    let now = Instant::now();
    assert!(!is_due(Some(now), now, Duration::from_secs(10)));
}

#[test]
fn is_due_once_the_interval_has_fully_elapsed() {
    let now = Instant::now();
    let last = now.checked_sub(Duration::from_secs(20)).unwrap();
    assert!(is_due(Some(last), now, Duration::from_secs(10)));
}

#[test]
fn is_not_due_just_before_the_interval_elapses() {
    let now = Instant::now();
    let last = now.checked_sub(Duration::from_secs(5)).unwrap();
    assert!(!is_due(Some(last), now, Duration::from_secs(10)));
}

#[test]
fn is_due_exactly_at_the_interval_boundary() {
    let now = Instant::now();
    let last = now.checked_sub(Duration::from_secs(10)).unwrap();
    assert!(is_due(Some(last), now, Duration::from_secs(10)));
}

fn job_with_dirs(id: &str, src: &std::path::Path, dest: &std::path::Path) -> Job {
    Job {
        id: id.to_string(),
        desc: "desc".to_string(),
        src: src.display().to_string(),
        dest: dest.display().to_string(),
        delete: false,
        skip: Vec::new(),
        no_confirm: true,
        interval: Some(1),
    }
}

#[test]
fn run_job_with_no_changes_succeeds_without_syncing() {
    let src = tempfile::tempdir().unwrap();
    let dest = tempfile::tempdir().unwrap();
    let job = job_with_dirs("no-op", src.path(), dest.path());

    run_job(&job).expect("run_job");
}

#[test]
fn run_job_with_changes_actually_syncs() {
    let src = tempfile::tempdir().unwrap();
    let dest = tempfile::tempdir().unwrap();
    std::fs::write(src.path().join("a.txt"), "hello").unwrap();
    let job = job_with_dirs("has-changes", src.path(), dest.path());

    run_job(&job).expect("run_job");

    assert_eq!(
        std::fs::read_to_string(dest.path().join("a.txt")).unwrap(),
        "hello"
    );
}

#[test]
fn poll_once_runs_a_due_job_and_records_when_it_ran() {
    let src = tempfile::tempdir().unwrap();
    let dest = tempfile::tempdir().unwrap();
    std::fs::write(src.path().join("a.txt"), "hello").unwrap();
    let job = job_with_dirs("due", src.path(), dest.path());

    let jobs = vec![(&job, Duration::from_secs(10))];
    let mut last_run: HashMap<String, Instant> = HashMap::new();
    let now = Instant::now();

    poll_once(&jobs, &mut last_run, now);

    assert_eq!(last_run.get(&job.id).copied(), Some(now));
    assert_eq!(
        std::fs::read_to_string(dest.path().join("a.txt")).unwrap(),
        "hello"
    );
}

#[test]
fn poll_once_skips_a_job_that_is_not_due_yet() {
    let src = tempfile::tempdir().unwrap();
    let dest = tempfile::tempdir().unwrap();
    let job = job_with_dirs("not-due", src.path(), dest.path());

    let jobs = vec![(&job, Duration::from_secs(3600))];
    let now = Instant::now();
    let mut last_run: HashMap<String, Instant> = HashMap::new();
    last_run.insert(job.id.clone(), now);

    poll_once(&jobs, &mut last_run, now);

    // Unchanged: `poll_once` must not have re-run (and re-recorded) it.
    assert_eq!(last_run.get(&job.id).copied(), Some(now));
}

#[test]
fn poll_once_records_the_run_even_when_the_job_fails() {
    let missing_src = std::env::temp_dir().join(format!(
        "pulley-daemon-poll-once-missing-src-{}",
        std::process::id()
    ));
    let dest = tempfile::tempdir().unwrap();
    let job = job_with_dirs("failing", &missing_src, dest.path());

    let jobs = vec![(&job, Duration::from_secs(10))];
    let mut last_run: HashMap<String, Instant> = HashMap::new();
    let now = Instant::now();

    poll_once(&jobs, &mut last_run, now);

    // A failed run still counts as an attempt, so a broken job doesn't
    // retry every tick forever.
    assert_eq!(last_run.get(&job.id).copied(), Some(now));
}

#[test]
fn run_job_errors_when_the_source_directory_does_not_exist() {
    let missing_src = std::env::temp_dir().join(format!(
        "pulley-daemon-test-missing-src-{}",
        std::process::id()
    ));
    let dest = tempfile::tempdir().unwrap();
    let job = job_with_dirs("bad-src", &missing_src, dest.path());

    assert!(run_job(&job).is_err());
}
