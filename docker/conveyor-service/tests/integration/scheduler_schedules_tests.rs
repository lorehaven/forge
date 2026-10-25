//! `scheduler::schedules` - the clock side of `on.schedule`. Raw SQL against a real Postgres plus a
//! real repository and worker, so these need `CONVEYOR_TEST_DATABASE_URL` - see `support.rs`.

use crate::support::{Origin, database, register_repo, skipped};
use conveyor_pipeline::Schedule;
use conveyor_service::config::ConveyorConfig;
use conveyor_service::domain::{Run, Status, Trigger};
use conveyor_service::executors::NativeExecutor;
use conveyor_service::providers::Providers;
use conveyor_service::scheduler::queue::{self, NewRun};
use conveyor_service::scheduler::schedules::{self, delivery_id};
use conveyor_service::scheduler::spawn_pool;
use quench_core::time::{Duration, Time};
use quench_db::prelude::{Database, Db};
use std::sync::Arc;

const NIGHTLY: &str = r#"
on = { push = ["master"], schedule = ["0 0 * * *"], timezone = "Europe/Warsaw" }

[[stage]]
name = "build"
when = "event != 'schedule'"
[[stage.job]]
name = "compile"
steps = ["echo building"]

[[stage]]
name = "security"
when = "event == 'schedule'"
[[stage.job]]
name = "nightly"
steps = ["echo scanning"]
"#;

fn nightly(expression: &str) -> Schedule {
    Schedule {
        expression: expression.to_string(),
        timezone: "UTC".to_string(),
    }
}

fn start_scheduler(db: &Db) -> quench_core::tmp::TempDir {
    let work = quench_core::tmp::TempDir::new().expect("temp dir");
    let config = ConveyorConfig {
        work_dir: work.path().to_path_buf(),
        max_concurrent_runs: 1,
        default_job_timeout_secs: 60,
        checkout_timeout_secs: 60,
        ..ConveyorConfig::default()
    };
    spawn_pool(
        db.clone(),
        config,
        Arc::new(NativeExecutor::new()),
        Arc::new(Providers::from_env()),
    );
    work
}

async fn settle(db: &Db, run_id: &str) -> Run {
    for _ in 0..600 {
        let run = queue::read_run(db, run_id).await.unwrap().unwrap();
        if run.status.is_terminal() {
            return run;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    panic!("the scheduler never finished the run");
}

async fn push_run(db: &Db, repo_id: &str, git_ref: &str, sha: &str) -> String {
    queue::enqueue(
        db,
        &NewRun {
            repo_id: repo_id.to_string(),
            trigger: Trigger::Push,
            git_ref: git_ref.to_string(),
            sha: sha.to_string(),
            message: None,
            delivery_id: None,
            resumed_from: None,
        },
    )
    .await
    .unwrap()
    .run()
    .id
    .clone()
}

const fn days(n: i64) -> Duration {
    Duration::seconds(n * 86_400)
}

#[quench_test::test]
async fn sync_adds_new_schedules_keeps_known_ones_and_drops_withdrawn_ones() {
    let Some((db, _guard)) = database().await else {
        return skipped("sync_adds_new_schedules_keeps_known_ones_and_drops_withdrawn_ones");
    };
    let repo = register_repo(&db, "sync", "file:///nowhere").await;
    let now = Time::now();

    let (added, removed) = schedules::sync(&db, &repo.id, &[nightly("0 0 * * *"), nightly("@hourly")], now)
        .await
        .unwrap();
    assert_eq!((added, removed), (2, 0));
    let stored = schedules::list(&db, &repo.id).await.unwrap();
    assert_eq!(stored.len(), 2);
    assert!(stored.iter().all(|s| s.next_run_at > now && s.last_run_at.is_none()));
    let first_ids: Vec<_> = stored.iter().map(|s| s.id.clone()).collect();

    // Declaring the same thing again changes nothing - in particular it must not reset the due time.
    let (added, removed) = schedules::sync(&db, &repo.id, &[nightly("0 0 * * *"), nightly("@hourly")], now + days(1))
        .await
        .unwrap();
    assert_eq!((added, removed), (0, 0));
    let again = schedules::list(&db, &repo.id).await.unwrap();
    assert_eq!(again.iter().map(|s| s.id.clone()).collect::<Vec<_>>(), first_ids);
    assert_eq!(again, stored, "an unchanged schedule must keep its place in time");

    // Withdrawing one removes just that one; a different zone is a different schedule.
    let (added, removed) = schedules::sync(
        &db,
        &repo.id,
        &[Schedule { expression: "0 0 * * *".into(), timezone: "Europe/Warsaw".into() }],
        now,
    )
    .await
    .unwrap();
    assert_eq!((added, removed), (1, 2));
    let left = schedules::list(&db, &repo.id).await.unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].timezone, "Europe/Warsaw");

    schedules::sync(&db, &repo.id, &[], now).await.unwrap();
    assert!(schedules::list(&db, &repo.id).await.unwrap().is_empty());
}

#[quench_test::test]
async fn a_default_branch_run_registers_the_pipelines_schedule_and_a_due_one_runs_only_its_own_stages() {
    let Some((db, _guard)) = database().await else {
        return skipped("a_default_branch_run_registers_the_pipelines_schedule_and_a_due_one_runs_only_its_own_stages");
    };
    let origin = Origin::with_pipeline(NIGHTLY);
    let repo = register_repo(&db, "nightly", &origin.url()).await;
    let _work = start_scheduler(&db);

    // Nothing is scheduled until a default-branch run has read the pipeline.
    assert!(schedules::list(&db, &repo.id).await.unwrap().is_empty());
    let push = push_run(&db, &repo.id, "refs/heads/master", &origin.sha).await;
    let push = settle(&db, &push).await;
    assert_eq!(push.status, Status::Success, "{:?}", push.error);

    let stored = schedules::list(&db, &repo.id).await.unwrap();
    assert_eq!(stored.len(), 1, "the push run should have registered the schedule");
    assert_eq!((stored[0].expression.as_str(), stored[0].timezone.as_str()), ("0 0 * * *", "Europe/Warsaw"));
    // The push itself ran the build stage and not the scheduled one.
    let jobs = queue::list_jobs(&db, &push.id).await.unwrap();
    let status_of = |name: &str| jobs.iter().find(|j| j.name == name).map(|j| j.status);
    assert_eq!(status_of("compile"), Some(Status::Success));
    assert_ne!(status_of("nightly"), Some(Status::Success));

    // Not due yet: nothing happens.
    let tick = schedules::fire_due(&db, Time::now()).await.unwrap();
    assert_eq!(tick, schedules::Tick::default());

    // Two days later it is due: one run, of kind `schedule`, on the default branch.
    let later = Time::now() + days(2);
    let tick = schedules::fire_due(&db, later).await.unwrap();
    assert_eq!((tick.started, tick.duplicates), (1, 0));

    let runs = queue::list_runs(&db, Some(&repo.id), 10).await.unwrap();
    let scheduled = runs.iter().find(|r| r.trigger == Trigger::Schedule).expect("a scheduled run");
    assert_eq!(scheduled.git_ref, "refs/heads/master");
    assert_eq!(scheduled.sha, origin.sha);
    let finished = settle(&db, &scheduled.id).await;
    assert_eq!(finished.status, Status::Success, "{:?}", finished.error);

    let jobs = queue::list_jobs(&db, &scheduled.id).await.unwrap();
    let status_of = |name: &str| jobs.iter().find(|j| j.name == name).map(|j| j.status);
    assert_eq!(status_of("nightly"), Some(Status::Success));
    assert_ne!(status_of("compile"), Some(Status::Success), "the scheduled run must not redo the build");

    // It advanced past `later`, so asking again at the same instant starts nothing.
    let stored = schedules::list(&db, &repo.id).await.unwrap();
    assert!(stored[0].next_run_at > later);
    assert!(stored[0].last_run_at.is_some());
    assert_eq!(schedules::fire_due(&db, later).await.unwrap(), schedules::Tick::default());
}

#[quench_test::test]
async fn the_same_firing_is_never_started_twice() {
    let Some((db, _guard)) = database().await else {
        return skipped("the_same_firing_is_never_started_twice");
    };
    let origin = Origin::with_pipeline(NIGHTLY);
    let repo = register_repo(&db, "dedupe", &origin.url()).await;
    schedules::sync(&db, &repo.id, &[nightly("0 0 * * *")], Time::now()).await.unwrap();
    let stored = schedules::list(&db, &repo.id).await.unwrap().remove(0);

    let later = Time::now() + days(2);
    assert_eq!(schedules::fire_due(&db, later).await.unwrap().started, 1);

    // Simulate a replica that enqueued the run but crashed before advancing the schedule (or two
    // replicas racing): the schedule is due again for the *same* firing.
    db.execute(&format!(
        "UPDATE conveyor.schedules SET next_run_at = to_timestamp({}) WHERE id = '{}'",
        stored.next_run_at.timestamp(),
        stored.id
    ))
    .await
    .unwrap();
    let tick = schedules::fire_due(&db, later).await.unwrap();
    assert_eq!((tick.started, tick.duplicates), (0, 1), "{tick:?}");

    let scheduled = queue::list_runs(&db, Some(&repo.id), 10)
        .await
        .unwrap()
        .into_iter()
        .filter(|r| r.trigger == Trigger::Schedule)
        .count();
    assert_eq!(scheduled, 1);
    assert_eq!(delivery_id(&stored.id, stored.next_run_at), format!("schedule:{}:{}", stored.id, stored.next_run_at.timestamp()));
}

#[quench_test::test]
async fn an_outage_over_several_firings_starts_one_catch_up_run() {
    let Some((db, _guard)) = database().await else {
        return skipped("an_outage_over_several_firings_starts_one_catch_up_run");
    };
    let origin = Origin::with_pipeline(NIGHTLY);
    let repo = register_repo(&db, "catchup", &origin.url()).await;
    schedules::sync(&db, &repo.id, &[nightly("0 0 * * *")], Time::now()).await.unwrap();

    // The service comes back a week later.
    let week = Time::now() + days(7);
    let tick = schedules::fire_due(&db, week).await.unwrap();
    assert_eq!(tick.started, 1, "seven missed nights are one catch-up run, not seven");
    let stored = schedules::list(&db, &repo.id).await.unwrap();
    assert!(stored[0].next_run_at > week);
    assert!(stored[0].next_run_at <= week + days(1) + Duration::seconds(60));
}

#[quench_test::test]
async fn a_disabled_repository_is_skipped_but_its_schedule_keeps_ticking_forward() {
    let Some((db, _guard)) = database().await else {
        return skipped("a_disabled_repository_is_skipped_but_its_schedule_keeps_ticking_forward");
    };
    let origin = Origin::with_pipeline(NIGHTLY);
    let repo = register_repo(&db, "disabled", &origin.url()).await;
    schedules::sync(&db, &repo.id, &[nightly("0 0 * * *")], Time::now()).await.unwrap();
    db.execute(&format!("UPDATE conveyor.repos SET enabled = FALSE WHERE id = '{}'", repo.id))
        .await
        .unwrap();

    let later = Time::now() + days(2);
    let tick = schedules::fire_due(&db, later).await.unwrap();
    assert_eq!((tick.started, tick.skipped), (0, 1));
    assert!(queue::list_runs(&db, Some(&repo.id), 10).await.unwrap().is_empty());
    // Advanced, so re-enabling later does not fire a stale night.
    assert!(schedules::list(&db, &repo.id).await.unwrap()[0].next_run_at > later);
}

#[quench_test::test]
async fn an_unreachable_repository_leaves_the_schedule_due_for_the_next_tick() {
    let Some((db, _guard)) = database().await else {
        return skipped("an_unreachable_repository_leaves_the_schedule_due_for_the_next_tick");
    };
    let repo = register_repo(&db, "unreachable", "file:///definitely/not/a/repository").await;
    schedules::sync(&db, &repo.id, &[nightly("0 0 * * *")], Time::now()).await.unwrap();
    let before = schedules::list(&db, &repo.id).await.unwrap().remove(0);

    let tick = schedules::fire_due(&db, Time::now() + days(2)).await.unwrap();
    assert_eq!((tick.started, tick.retried), (0, 1), "{tick:?}");
    let after = schedules::list(&db, &repo.id).await.unwrap().remove(0);
    assert_eq!(after.next_run_at, before.next_run_at, "a failed start must not consume the firing");
}

#[quench_test::test]
async fn a_feature_branch_cannot_register_a_schedule() {
    let Some((db, _guard)) = database().await else {
        return skipped("a_feature_branch_cannot_register_a_schedule");
    };
    let origin = Origin::with_pipeline(NIGHTLY);
    crate::support::git(origin.dir.path(), &["checkout", "--quiet", "-b", "feature"]);
    let repo = register_repo(&db, "feature", &origin.url()).await;
    let _work = start_scheduler(&db);

    // `on.push` only lists master, so the run is skipped - but the pipeline was read, and a schedule
    // declared on a non-default branch must still not be registered.
    let run = push_run(&db, &repo.id, "refs/heads/feature", &origin.sha).await;
    settle(&db, &run).await;
    assert!(schedules::list(&db, &repo.id).await.unwrap().is_empty());
}
