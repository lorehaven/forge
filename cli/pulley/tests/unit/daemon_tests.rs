use pulley::config::{Config, Job};
use pulley::daemon::{
    Event, Watch, format_duration, group_by_dest, is_due, poll_once, run, run_job, serve,
};
use pulley::source::{self, Availability};
use std::cell::Cell;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::{Duration, Instant};

#[quench_test::test]
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
            marker: None,
        }],
    };
    run(&config).expect("run");
}

#[quench_test::test]
fn run_returns_immediately_for_an_empty_job_list() {
    let config = Config { jobs: Vec::new() };
    run(&config).expect("run");
}

#[quench_test::test]
fn is_due_when_never_run_before() {
    assert!(is_due(None, Instant::now(), Duration::from_secs(10)));
}

#[quench_test::test]
fn is_not_due_immediately_after_running() {
    let now = Instant::now();
    assert!(!is_due(Some(now), now, Duration::from_secs(10)));
}

#[quench_test::test]
fn is_due_once_the_interval_has_fully_elapsed() {
    let now = Instant::now();
    let last = now.checked_sub(Duration::from_secs(20)).unwrap();
    assert!(is_due(Some(last), now, Duration::from_secs(10)));
}

#[quench_test::test]
fn is_not_due_just_before_the_interval_elapses() {
    let now = Instant::now();
    let last = now.checked_sub(Duration::from_secs(5)).unwrap();
    assert!(!is_due(Some(last), now, Duration::from_secs(10)));
}

#[quench_test::test]
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
        marker: None,
    }
}

#[quench_test::test]
fn run_job_with_no_changes_succeeds_without_syncing() {
    let src = quench_core::tmp::TempDir::new().unwrap();
    let dest = quench_core::tmp::TempDir::new().unwrap();
    let job = job_with_dirs("no-op", src.path(), dest.path());

    run_job(&job).expect("run_job");
}

#[quench_test::test]
fn run_job_with_changes_actually_syncs() {
    let src = quench_core::tmp::TempDir::new().unwrap();
    let dest = quench_core::tmp::TempDir::new().unwrap();
    std::fs::write(src.path().join("a.txt"), "hello").unwrap();
    let job = job_with_dirs("has-changes", src.path(), dest.path());

    run_job(&job).expect("run_job");

    assert_eq!(
        std::fs::read_to_string(dest.path().join("a.txt")).unwrap(),
        "hello"
    );
}

/// A pass with the real check and the real sync.
fn pass(jobs: &[(&Job, Duration)], watch: &mut Watch, now: Instant) -> Vec<Event> {
    poll_once(jobs, watch, now, &source::check, &run_job)
}

#[quench_test::test]
fn poll_once_runs_a_due_job_and_records_when_it_ran() {
    let src = quench_core::tmp::TempDir::new().unwrap();
    let dest = quench_core::tmp::TempDir::new().unwrap();
    std::fs::write(src.path().join("a.txt"), "hello").unwrap();
    let job = job_with_dirs("due", src.path(), dest.path());

    let jobs = vec![(&job, Duration::from_secs(10))];
    let mut watch = Watch::default();
    let now = Instant::now();

    assert_eq!(pass(&jobs, &mut watch, now), []);

    assert_eq!(watch.last_run.get(&job.id).copied(), Some(now));
    assert_eq!(
        std::fs::read_to_string(dest.path().join("a.txt")).unwrap(),
        "hello"
    );
}

#[quench_test::test]
fn poll_once_skips_a_job_that_is_not_due_yet() {
    let src = quench_core::tmp::TempDir::new().unwrap();
    let dest = quench_core::tmp::TempDir::new().unwrap();
    let job = job_with_dirs("not-due", src.path(), dest.path());

    let jobs = vec![(&job, Duration::from_secs(3600))];
    let now = Instant::now();
    let mut watch = Watch::default();
    watch.last_run.insert(job.id.clone(), now);

    assert_eq!(pass(&jobs, &mut watch, now), []);

    // Unchanged: `poll_once` must not have re-run (and re-recorded) it, nor looked at its source.
    assert_eq!(watch.last_run.get(&job.id).copied(), Some(now));
    assert!(watch.availability.is_empty());
}

#[quench_test::test]
fn a_sync_that_fails_is_recorded_and_reported_and_not_retried_every_tick() {
    let src = quench_core::tmp::TempDir::new().unwrap();
    let dest = quench_core::tmp::TempDir::new().unwrap();
    std::fs::write(src.path().join("a.txt"), "hello").unwrap();
    // A destination that cannot be made: its parent is a file.
    let blocker = dest.path().join("file");
    std::fs::write(&blocker, "x").unwrap();
    let job = job_with_dirs("failing", src.path(), &blocker.join("sub"));

    let jobs = vec![(&job, Duration::from_secs(10))];
    let mut watch = Watch::default();
    let now = Instant::now();

    let events = pass(&jobs, &mut watch, now);

    assert!(
        matches!(&events[..], [Event::Failed { job, .. }] if job == "failing"),
        "{events:?}"
    );
    // A failed run still counts as an attempt, so a broken job doesn't retry every tick forever.
    assert_eq!(watch.last_run.get(&job.id).copied(), Some(now));
}

// ── A source that is not there ──────────────────────────────────────────────────────────────────────────────────

/// A job with the check and the sync replaced, so the passes can be followed without rsync: what the check will answer next,
/// how often it was asked, how often a sync started.
struct Script {
    answers: std::cell::RefCell<Vec<Availability>>,
    probes: Cell<u32>,
    runs: Cell<u32>,
}

impl Script {
    const fn new(answers: Vec<Availability>) -> Self {
        Self {
            answers: std::cell::RefCell::new(answers),
            probes: Cell::new(0),
            runs: Cell::new(0),
        }
    }

    fn pass(&self, job: &Job, interval: u64, watch: &mut Watch, now: Instant) -> Vec<Event> {
        let jobs = vec![(job, Duration::from_secs(interval))];
        poll_once(
            &jobs,
            watch,
            now,
            &|_| {
                self.probes.set(self.probes.get() + 1);
                let mut answers = self.answers.borrow_mut();
                if answers.len() > 1 {
                    answers.remove(0)
                } else {
                    answers[0].clone()
                }
            },
            &|_| {
                self.runs.set(self.runs.get() + 1);
                Ok(())
            },
        )
    }
}

fn idle_job() -> Job {
    Job {
        id: "lorien".to_string(),
        desc: "desc".to_string(),
        src: "me@lorien:/data".to_string(),
        dest: "/backup".to_string(),
        delete: true,
        skip: Vec::new(),
        no_confirm: true,
        interval: Some(1800),
        marker: Some("projects".to_string()),
    }
}

fn down() -> Availability {
    Availability::Unreachable("ssh: connect to host lorien port 22: No route to host".into())
}

#[quench_test::test]
fn a_source_that_is_not_there_is_not_synced_and_is_said_once() {
    let job = idle_job();
    let script = Script::new(vec![down()]);
    let mut watch = Watch::default();
    let t0 = Instant::now();

    let first = script.pass(&job, 1800, &mut watch, t0);
    assert_eq!(
        first,
        [Event::Unavailable {
            job: "lorien".into(),
            why: down()
        }]
    );
    assert_eq!(
        script.runs.get(),
        0,
        "nothing is synced, so nothing is deleted"
    );
    assert!(watch.last_run.is_empty(), "no sync started");

    // Looked at again no sooner than a minute, however often the loop ticks.
    assert_eq!(
        script.pass(&job, 1800, &mut watch, t0 + Duration::from_secs(1)),
        []
    );
    assert_eq!(
        script.pass(&job, 1800, &mut watch, t0 + Duration::from_secs(59)),
        []
    );
    assert_eq!(script.probes.get(), 1);

    // After that it is looked at, and still not there: not news.
    assert_eq!(
        script.pass(&job, 1800, &mut watch, t0 + Duration::from_secs(61)),
        []
    );
    assert_eq!(
        script.pass(&job, 1800, &mut watch, t0 + Duration::from_secs(125)),
        []
    );
    assert_eq!(script.probes.get(), 3);
    assert_eq!(script.runs.get(), 0);
}

#[quench_test::test]
fn a_different_way_of_not_being_there_is_said_once_more() {
    let job = idle_job();
    let not_ready = Availability::NotReady("`projects` is not in /data".into());
    let script = Script::new(vec![down(), not_ready.clone()]);
    let mut watch = Watch::default();
    let t0 = Instant::now();

    assert_eq!(script.pass(&job, 1800, &mut watch, t0).len(), 1);
    // The host answers now, but its drive is not mounted: that is news.
    assert_eq!(
        script.pass(&job, 1800, &mut watch, t0 + Duration::from_secs(61)),
        [Event::Unavailable {
            job: "lorien".into(),
            why: not_ready
        }]
    );
    assert_eq!(script.runs.get(), 0);
}

#[quench_test::test]
fn it_syncs_as_soon_as_the_source_is_back_not_after_the_interval() {
    let job = idle_job();
    let script = Script::new(vec![down(), down(), Availability::Ready]);
    let mut watch = Watch::default();
    let t0 = Instant::now();

    script.pass(&job, 1800, &mut watch, t0);
    script.pass(&job, 1800, &mut watch, t0 + Duration::from_secs(61));
    // Back a little over two minutes in, with an interval of half an hour.
    let back = t0 + Duration::from_secs(125);
    let events = script.pass(&job, 1800, &mut watch, back);

    assert_eq!(
        events,
        [Event::Recovered {
            job: "lorien".into()
        }]
    );
    assert_eq!(script.runs.get(), 1);
    assert_eq!(watch.last_run.get("lorien").copied(), Some(back));

    // And then it is on its interval again: nothing until half an hour after.
    assert_eq!(
        script.pass(&job, 1800, &mut watch, back + Duration::from_secs(600)),
        []
    );
    assert_eq!(script.runs.get(), 1);
    assert_eq!(
        script.pass(&job, 1800, &mut watch, back + Duration::from_mins(30)),
        []
    );
    assert_eq!(script.runs.get(), 2);
}

#[quench_test::test]
fn a_source_that_is_always_there_is_never_news() {
    let job = idle_job();
    let script = Script::new(vec![Availability::Ready]);
    let mut watch = Watch::default();
    let t0 = Instant::now();

    assert_eq!(script.pass(&job, 10, &mut watch, t0), []);
    assert_eq!(
        script.pass(&job, 10, &mut watch, t0 + Duration::from_secs(10)),
        []
    );
    assert_eq!(script.runs.get(), 2);
}

#[quench_test::test]
fn a_short_interval_is_not_stretched_to_the_recheck() {
    let job = idle_job();
    let script = Script::new(vec![down()]);
    let mut watch = Watch::default();
    let t0 = Instant::now();

    script.pass(&job, 10, &mut watch, t0);
    script.pass(&job, 10, &mut watch, t0 + Duration::from_secs(11));
    assert_eq!(
        script.probes.get(),
        2,
        "a ten second job looks every ten seconds"
    );
}

// ── A host that goes away in the middle of a sync ────────────────────────────────────────────────────────────────

#[quench_test::test]
fn a_sync_that_fails_because_the_host_went_away_resumes_as_soon_as_it_is_back() {
    let job = idle_job();
    let answers = std::cell::RefCell::new(vec![
        Availability::Ready, // before the sync
        down(),              // after it failed
        Availability::Ready, // the host is back
    ]);
    let runs = Cell::new(0);
    let jobs = vec![(&job, Duration::from_secs(3600))];
    let mut watch = Watch::default();
    let t0 = Instant::now();
    let probe = |_: &Job| {
        let mut answers = answers.borrow_mut();
        if answers.len() > 1 {
            answers.remove(0)
        } else {
            answers[0].clone()
        }
    };
    let run_it = |_: &Job| {
        runs.set(runs.get() + 1);
        if runs.get() == 1 {
            Err("rsync exited with exit status: 255: ssh: connection timed out".into())
        } else {
            Ok(())
        }
    };

    // The sync fails, and the host is not there to ask: that is an absence, said once, and the job stays due.
    let events = poll_once(&jobs, &mut watch, t0, &probe, &run_it);
    assert_eq!(
        events,
        [Event::Unavailable {
            job: "lorien".into(),
            why: down()
        }]
    );
    assert!(
        watch.last_run.is_empty(),
        "the work is not done, so it is due again"
    );

    // Looked at again no sooner than a minute later.
    assert_eq!(
        poll_once(
            &jobs,
            &mut watch,
            t0 + Duration::from_secs(30),
            &probe,
            &run_it
        ),
        []
    );
    assert_eq!(runs.get(), 1);

    // Back after a minute and a bit, with an interval of an hour: it resumes now, not an hour after the failure.
    let back = t0 + Duration::from_secs(61);
    let events = poll_once(&jobs, &mut watch, back, &probe, &run_it);
    assert_eq!(
        events,
        [Event::Recovered {
            job: "lorien".into()
        }]
    );
    assert_eq!(runs.get(), 2);
    assert_eq!(watch.last_run.get("lorien").copied(), Some(back));
}

#[quench_test::test]
fn a_sync_that_fails_with_the_host_still_there_is_a_failure() {
    let job = idle_job();
    let jobs = vec![(&job, Duration::from_secs(3600))];
    let mut watch = Watch::default();
    let now = Instant::now();

    let events = poll_once(&jobs, &mut watch, now, &|_| Availability::Ready, &|_| {
        Err("rsync exited with exit status: 23: some files could not be read".into())
    });

    assert!(
        matches!(&events[..], [Event::Failed { error, .. }] if error.contains("some files")),
        "{events:?}"
    );
    // Counted as an attempt: it is tried again at its interval, not on every tick.
    assert_eq!(watch.last_run.get("lorien").copied(), Some(now));
}

// ── Jobs that run at the same time ───────────────────────────────────────────────────────────────────────────────

fn job_to(id: &str, dest: &str) -> (Job, Duration) {
    (
        Job {
            id: id.to_string(),
            desc: "desc".to_string(),
            src: "/src".to_string(),
            dest: dest.to_string(),
            delete: false,
            skip: Vec::new(),
            no_confirm: true,
            interval: Some(60),
            marker: None,
        },
        Duration::from_secs(60),
    )
}

#[quench_test::test]
fn jobs_that_write_to_the_same_destination_share_a_worker_and_the_rest_do_not() {
    let jobs = vec![
        job_to("a", "/backup/x"),
        job_to("b", "/backup/y"),
        job_to("c", "/backup/x/"),
    ];

    let groups = group_by_dest(&jobs);

    let ids: Vec<Vec<&str>> = groups
        .iter()
        .map(|group| group.iter().map(|(job, _)| job.id.as_str()).collect())
        .collect();
    // In the order given, a trailing slash not making a different destination.
    assert_eq!(ids, [vec!["a", "c"], vec!["b"]]);
    assert!(group_by_dest(&[]).is_empty());
}

#[quench_test::test]
fn a_job_that_takes_long_does_not_hold_up_the_others() {
    let slow = job_to("slow", "/backup/slow");
    let mut fast = job_to("fast", "/backup/fast");
    fast.1 = Duration::from_millis(20);
    let groups = group_by_dest(&[slow, fast]);

    let stop = AtomicBool::new(false);
    let fast_runs = AtomicU32::new(0);
    let slow_running = AtomicBool::new(false);
    let fast_ran_meanwhile = AtomicBool::new(false);

    std::thread::scope(|scope| {
        scope.spawn(|| {
            serve(
                &groups,
                &stop,
                Duration::from_millis(5),
                &|_| Availability::Ready,
                &|job| {
                    if job.id == "slow" {
                        slow_running.store(true, Ordering::SeqCst);
                        std::thread::sleep(Duration::from_millis(500));
                        slow_running.store(false, Ordering::SeqCst);
                    } else {
                        fast_runs.fetch_add(1, Ordering::SeqCst);
                        if slow_running.load(Ordering::SeqCst) {
                            fast_ran_meanwhile.store(true, Ordering::SeqCst);
                        }
                    }
                    Ok(())
                },
            )
            .expect("serve");
        });
        std::thread::sleep(Duration::from_millis(300));
        // Half way through the slow job, the fast one has gone round several times.
        assert!(slow_running.load(Ordering::SeqCst));
        assert!(
            fast_runs.load(Ordering::SeqCst) >= 3,
            "{}",
            fast_runs.load(Ordering::SeqCst)
        );
        stop.store(true, Ordering::SeqCst);
    });

    assert!(fast_ran_meanwhile.load(Ordering::SeqCst));
}

#[quench_test::test]
fn a_worker_that_dies_takes_the_daemon_down_so_it_can_be_restarted() {
    let groups = group_by_dest(&[job_to("bad", "/backup/bad"), job_to("fine", "/backup/fine")]);
    let stop = AtomicBool::new(false);

    let result = serve(
        &groups,
        &stop,
        Duration::from_millis(5),
        &|_| Availability::Ready,
        &|job| {
            assert!(job.id != "bad", "this job panics");
            Ok(())
        },
    );

    // The other worker was stopped too, instead of running on with a job unwatched.
    assert!(result.is_err());
    assert!(stop.load(Ordering::SeqCst));
}

#[quench_test::test]
fn durations_are_whole_seconds_and_only_the_units_that_matter() {
    assert_eq!(format_duration(Duration::from_millis(400)), "0s");
    assert_eq!(format_duration(Duration::from_secs(45)), "45s");
    assert_eq!(format_duration(Duration::from_secs(60)), "1m00s");
    assert_eq!(format_duration(Duration::from_secs(4 * 60 + 12)), "4m12s");
    assert_eq!(format_duration(Duration::from_secs(59 * 60 + 59)), "59m59s");
    assert_eq!(format_duration(Duration::from_secs(3600)), "1h00m");
    assert_eq!(
        format_duration(Duration::from_secs(3 * 3600 + 5 * 60 + 9)),
        "3h05m"
    );
}

// ── The destination, against real rsync ─────────────────────────────────────────────────────────────────────────

fn deleting_job(src: &std::path::Path, dest: &std::path::Path) -> Job {
    Job {
        delete: true,
        ..job_with_dirs("guarded", src, dest)
    }
}

#[quench_test::test]
fn a_source_that_answers_but_lost_its_data_never_empties_the_destination() {
    // The host is up and the directory is there, but the drive that holds the data is not mounted: the source is empty. No
    // marker, no file anywhere: an empty source is enough to know.
    let src = quench_core::tmp::TempDir::new().unwrap();
    let dest = quench_core::tmp::TempDir::new().unwrap();
    std::fs::create_dir_all(dest.path().join("projects/forge")).unwrap();
    std::fs::write(dest.path().join("projects/forge/main.rs"), "fn main() {}").unwrap();
    std::fs::write(dest.path().join("notes.txt"), "keep me").unwrap();
    let job = deleting_job(src.path(), dest.path());

    let jobs = vec![(&job, Duration::from_secs(10))];
    let mut watch = Watch::default();
    let t0 = Instant::now();

    let events = pass(&jobs, &mut watch, t0);
    assert!(
        matches!(
            &events[..],
            [Event::Unavailable {
                why: Availability::NotReady(_),
                ..
            }]
        ),
        "{events:?}"
    );
    assert!(dest.path().join("projects/forge/main.rs").exists());
    assert_eq!(
        std::fs::read_to_string(dest.path().join("notes.txt")).unwrap(),
        "keep me"
    );

    // The drive is mounted: the data is there, and now the destination follows the source, deletions too.
    std::fs::write(src.path().join("new.txt"), "new").unwrap();
    let events = pass(&jobs, &mut watch, t0 + Duration::from_secs(61));
    assert_eq!(
        events,
        [Event::Recovered {
            job: "guarded".into()
        }]
    );
    assert_eq!(
        std::fs::read_to_string(dest.path().join("new.txt")).unwrap(),
        "new"
    );
    assert!(
        !dest.path().join("notes.txt").exists(),
        "gone at the source, gone at the backup"
    );
    assert!(!dest.path().join("projects").exists());
}

#[quench_test::test]
fn a_deletion_at_the_source_is_propagated_however_large() {
    // A whole project removed on purpose, thousands of files across directories: nothing about the amount holds it back.
    let src = quench_core::tmp::TempDir::new().unwrap();
    let dest = quench_core::tmp::TempDir::new().unwrap();
    std::fs::write(src.path().join("keep.txt"), "stays").unwrap();
    for d in 0..30 {
        let dir = src.path().join(format!("project/module{d}/src"));
        std::fs::create_dir_all(&dir).unwrap();
        for f in 0..100 {
            std::fs::write(dir.join(format!("file{f}.rs")), format!("{d}-{f}")).unwrap();
        }
    }
    let job = deleting_job(src.path(), dest.path());
    let jobs = vec![(&job, Duration::from_secs(10))];
    let mut watch = Watch::default();
    let t0 = Instant::now();

    assert_eq!(pass(&jobs, &mut watch, t0), []);
    assert!(dest.path().join("project/module29/src/file99.rs").exists());

    std::fs::remove_dir_all(src.path().join("project")).unwrap();
    assert_eq!(pass(&jobs, &mut watch, t0 + Duration::from_secs(10)), []);

    assert!(
        !dest.path().join("project").exists(),
        "3000 files deleted at the source, deleted at the backup"
    );
    assert_eq!(
        std::fs::read_to_string(dest.path().join("keep.txt")).unwrap(),
        "stays"
    );
}

#[quench_test::test]
fn run_job_errors_when_the_source_directory_does_not_exist() {
    let missing_src = std::env::temp_dir().join(format!(
        "pulley-daemon-test-missing-src-{}",
        std::process::id()
    ));
    let dest = quench_core::tmp::TempDir::new().unwrap();
    let job = job_with_dirs("bad-src", &missing_src, dest.path());

    assert!(run_job(&job).is_err());
}
