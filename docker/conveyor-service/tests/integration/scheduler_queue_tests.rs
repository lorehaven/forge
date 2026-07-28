//! The run queue, against a real Postgres.
//!
//! `FOR UPDATE SKIP LOCKED`, a partial unique index and `ON CONFLICT DO NOTHING`
//! are the whole design here, and none of them exist outside Postgres. A test
//! against anything else would be testing a different queue.

use crate::support::{database, register_repo, skipped};
use chrono::Utc;
use conveyor_service::domain::{Status, Trigger};
use conveyor_service::executors::{LogChunk, StepState, Stream};
use conveyor_service::scheduler::queue::{self, NewRun, PlannedJob};
use quench_db::prelude::{Database, Db};

fn new_run(repo_id: &str, sha: &str) -> NewRun {
    NewRun {
        repo_id: repo_id.to_string(),
        trigger: Trigger::Push,
        git_ref: "refs/heads/master".to_string(),
        sha: sha.to_string(),
        message: Some("a commit".to_string()),
        delivery_id: None,
    }
}

const SHA_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SHA_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

// ---------------------------------------------------------------------------
// Enqueueing
// ---------------------------------------------------------------------------

#[tokio::test]
async fn enqueue_creates_a_queued_run() {
    let Some((db, _guard)) = database().await else {
        return skipped("enqueue_creates_a_queued_run");
    };
    let repo = register_repo(&db, "one", "file:///nowhere").await;

    let enqueued = queue::enqueue(&db, &new_run(&repo.id, SHA_A))
        .await
        .expect("enqueue");

    assert!(enqueued.is_new());
    assert_eq!(enqueued.run().status, Status::Queued);
    assert_eq!(enqueued.run().sha, SHA_A);
    assert_eq!(enqueued.run().attempt, 0);
    assert!(enqueued.run().claimed_by.is_none());
}

#[tokio::test]
async fn a_redelivered_webhook_does_not_queue_a_second_run() {
    // A provider retries a webhook it did not get a prompt answer for, and a
    // second run of the same commit would double every side effect the first
    // one had.
    let Some((db, _guard)) = database().await else {
        return skipped("a_redelivered_webhook_does_not_queue_a_second_run");
    };
    let repo = register_repo(&db, "one", "file:///nowhere").await;

    let mut run = new_run(&repo.id, SHA_A);
    run.delivery_id = Some("delivery-1".to_string());

    let first = queue::enqueue(&db, &run).await.expect("enqueue");
    let second = queue::enqueue(&db, &run).await.expect("enqueue again");

    assert!(first.is_new());
    assert!(!second.is_new(), "the second delivery should be recognised");
    assert_eq!(first.run().id, second.run().id);

    assert_eq!(
        queue::list_runs(&db, None, 50).await.expect("list").len(),
        1
    );
}

#[tokio::test]
async fn manual_runs_of_the_same_commit_are_both_allowed() {
    // A manual run has no delivery to be a duplicate of; asking for the same
    // commit twice on purpose is a reasonable thing to do.
    let Some((db, _guard)) = database().await else {
        return skipped("manual_runs_of_the_same_commit_are_both_allowed");
    };
    let repo = register_repo(&db, "one", "file:///nowhere").await;

    let mut run = new_run(&repo.id, SHA_A);
    run.trigger = Trigger::Manual;

    queue::enqueue(&db, &run).await.expect("enqueue");
    queue::enqueue(&db, &run).await.expect("enqueue again");

    assert_eq!(
        queue::list_runs(&db, None, 50).await.expect("list").len(),
        2
    );
}

// ---------------------------------------------------------------------------
// Claiming
// ---------------------------------------------------------------------------

#[tokio::test]
async fn an_empty_queue_claims_nothing() {
    let Some((db, _guard)) = database().await else {
        return skipped("an_empty_queue_claims_nothing");
    };
    assert!(
        queue::claim_next(&db, "worker-1")
            .await
            .expect("claim")
            .is_none()
    );
}

#[tokio::test]
async fn claiming_takes_the_oldest_queued_run() {
    let Some((db, _guard)) = database().await else {
        return skipped("claiming_takes_the_oldest_queued_run");
    };
    let first_repo = register_repo(&db, "one", "file:///nowhere").await;
    let second_repo = register_repo(&db, "two", "file:///nowhere").await;

    let first = queue::enqueue(&db, &new_run(&first_repo.id, SHA_A))
        .await
        .expect("enqueue");
    queue::enqueue(&db, &new_run(&second_repo.id, SHA_B))
        .await
        .expect("enqueue");

    let claimed = queue::claim_next(&db, "worker-1")
        .await
        .expect("claim")
        .expect("something to claim");

    assert_eq!(claimed.id, first.run().id);
    assert_eq!(claimed.status, Status::Running);
    assert_eq!(claimed.claimed_by.as_deref(), Some("worker-1"));
    assert_eq!(claimed.attempt, 1, "claiming counts as an attempt");
    assert!(claimed.started_at.is_some());
}

#[tokio::test]
async fn a_repository_never_has_two_runs_in_flight() {
    // Two checkouts of the same repository at once would race on the registry
    // they push to, and on the workspace directory itself.
    let Some((db, _guard)) = database().await else {
        return skipped("a_repository_never_has_two_runs_in_flight");
    };
    let repo = register_repo(&db, "one", "file:///nowhere").await;

    queue::enqueue(&db, &new_run(&repo.id, SHA_A))
        .await
        .expect("enqueue");
    queue::enqueue(&db, &new_run(&repo.id, SHA_B))
        .await
        .expect("enqueue");

    assert!(
        queue::claim_next(&db, "worker-1")
            .await
            .expect("claim")
            .is_some()
    );
    assert!(
        queue::claim_next(&db, "worker-2")
            .await
            .expect("claim")
            .is_none(),
        "the second run of the same repository must wait"
    );
}

#[tokio::test]
async fn the_second_run_is_claimable_once_the_first_finishes() {
    let Some((db, _guard)) = database().await else {
        return skipped("the_second_run_is_claimable_once_the_first_finishes");
    };
    let repo = register_repo(&db, "one", "file:///nowhere").await;

    queue::enqueue(&db, &new_run(&repo.id, SHA_A))
        .await
        .expect("enqueue");
    let second = queue::enqueue(&db, &new_run(&repo.id, SHA_B))
        .await
        .expect("enqueue");

    let first = queue::claim_next(&db, "worker-1")
        .await
        .expect("claim")
        .expect("claimable");
    queue::finish_run(&db, &first.id, Status::Success, None)
        .await
        .expect("finish");

    let next = queue::claim_next(&db, "worker-2")
        .await
        .expect("claim")
        .expect("claimable now");
    assert_eq!(next.id, second.run().id);
}

#[tokio::test]
async fn the_database_refuses_a_second_running_run_for_one_repository() {
    // The claim query's `NOT EXISTS` and the claim are not one atomic act, so
    // this index is what actually holds the rule. Asserted directly, because a
    // race is not something a test can reliably provoke.
    let Some((db, _guard)) = database().await else {
        return skipped("the_database_refuses_a_second_running_run_for_one_repository");
    };
    let repo = register_repo(&db, "one", "file:///nowhere").await;

    let first = queue::enqueue(&db, &new_run(&repo.id, SHA_A))
        .await
        .expect("enqueue");
    let second = queue::enqueue(&db, &new_run(&repo.id, SHA_B))
        .await
        .expect("enqueue");

    let sql = |id: &str| format!("UPDATE conveyor.runs SET status = 'running' WHERE id = '{id}'");

    db.execute(&sql(&first.run().id)).await.expect("first");
    assert!(
        db.execute(&sql(&second.run().id)).await.is_err(),
        "a second running run for the same repository must be refused"
    );
}

#[tokio::test]
async fn a_disabled_repository_is_not_claimed() {
    let Some((db, _guard)) = database().await else {
        return skipped("a_disabled_repository_is_not_claimed");
    };
    let repo = register_repo(&db, "one", "file:///nowhere").await;
    queue::enqueue(&db, &new_run(&repo.id, SHA_A))
        .await
        .expect("enqueue");

    conveyor_service::scheduler::repos::set_enabled(&db, &repo.id, false)
        .await
        .expect("disable");

    assert!(
        queue::claim_next(&db, "worker-1")
            .await
            .expect("claim")
            .is_none(),
        "a disabled repository keeps its queued runs but does not build them"
    );
}

// ---------------------------------------------------------------------------
// Abandoned runs
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_run_whose_worker_died_goes_back_on_the_queue() {
    // Without this, one killed worker takes its repository out of service for
    // good: the run stays `running` and the index never lets another start.
    let Some((db, _guard)) = database().await else {
        return skipped("a_run_whose_worker_died_goes_back_on_the_queue");
    };
    let repo = register_repo(&db, "one", "file:///nowhere").await;
    queue::enqueue(&db, &new_run(&repo.id, SHA_A))
        .await
        .expect("enqueue");

    let claimed = queue::claim_next(&db, "doomed-worker")
        .await
        .expect("claim")
        .expect("claimable");

    // Age the claim past the threshold.
    db.execute(&format!(
        "UPDATE conveyor.runs SET claimed_at = NOW() - INTERVAL '1 hour' WHERE id = '{}'",
        claimed.id
    ))
    .await
    .expect("age the claim");

    assert_eq!(queue::requeue_stale(&db, 300).await.expect("requeue"), 1);

    let recovered = queue::claim_next(&db, "worker-2")
        .await
        .expect("claim")
        .expect("claimable again");
    assert_eq!(recovered.id, claimed.id);
    assert_eq!(recovered.attempt, 2, "the retry is counted");
}

#[tokio::test]
async fn a_run_with_a_fresh_heartbeat_is_left_alone() {
    let Some((db, _guard)) = database().await else {
        return skipped("a_run_with_a_fresh_heartbeat_is_left_alone");
    };
    let repo = register_repo(&db, "one", "file:///nowhere").await;
    queue::enqueue(&db, &new_run(&repo.id, SHA_A))
        .await
        .expect("enqueue");

    let claimed = queue::claim_next(&db, "worker-1")
        .await
        .expect("claim")
        .expect("claimable");
    queue::heartbeat(&db, &claimed.id, "worker-1")
        .await
        .expect("heartbeat");

    assert_eq!(queue::requeue_stale(&db, 300).await.expect("requeue"), 0);
}

// ---------------------------------------------------------------------------
// Cancellation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn cancelling_a_queued_run_ends_it_outright() {
    // Nothing is running, so there is nothing to wind down.
    let Some((db, _guard)) = database().await else {
        return skipped("cancelling_a_queued_run_ends_it_outright");
    };
    let repo = register_repo(&db, "one", "file:///nowhere").await;
    let run = queue::enqueue(&db, &new_run(&repo.id, SHA_A))
        .await
        .expect("enqueue");

    assert!(
        queue::request_cancel(&db, &run.run().id)
            .await
            .expect("cancel")
    );

    let after = queue::read_run(&db, &run.run().id)
        .await
        .expect("read")
        .expect("still there");
    assert_eq!(after.status, Status::Cancelled);
    assert!(after.finished_at.is_some());

    assert!(
        queue::claim_next(&db, "worker-1")
            .await
            .expect("claim")
            .is_none(),
        "a cancelled run must not then be picked up"
    );
}

#[tokio::test]
async fn cancelling_a_running_run_only_asks() {
    // The worker holding it is probably on another replica; it notices on its
    // next poll and tears the job down itself.
    let Some((db, _guard)) = database().await else {
        return skipped("cancelling_a_running_run_only_asks");
    };
    let repo = register_repo(&db, "one", "file:///nowhere").await;
    queue::enqueue(&db, &new_run(&repo.id, SHA_A))
        .await
        .expect("enqueue");
    let claimed = queue::claim_next(&db, "worker-1")
        .await
        .expect("claim")
        .expect("claimable");

    assert!(
        !queue::is_cancel_requested(&db, &claimed.id)
            .await
            .expect("flag")
    );
    assert!(
        queue::request_cancel(&db, &claimed.id)
            .await
            .expect("cancel")
    );
    assert!(
        queue::is_cancel_requested(&db, &claimed.id)
            .await
            .expect("flag")
    );

    let after = queue::read_run(&db, &claimed.id)
        .await
        .expect("read")
        .expect("still there");
    assert_eq!(after.status, Status::Running, "still winding down");
}

#[tokio::test]
async fn a_finished_run_cannot_be_cancelled() {
    let Some((db, _guard)) = database().await else {
        return skipped("a_finished_run_cannot_be_cancelled");
    };
    let repo = register_repo(&db, "one", "file:///nowhere").await;
    let run = queue::enqueue(&db, &new_run(&repo.id, SHA_A))
        .await
        .expect("enqueue");
    queue::finish_run(&db, &run.run().id, Status::Success, None)
        .await
        .expect("finish");

    assert!(
        !queue::request_cancel(&db, &run.run().id)
            .await
            .expect("cancel"),
        "there is nothing left to cancel"
    );
}

// ---------------------------------------------------------------------------
// Jobs, steps and logs
// ---------------------------------------------------------------------------

async fn a_job(db: &Db) -> (String, String) {
    let repo = register_repo(db, "one", "file:///nowhere").await;
    let run = queue::enqueue(db, &new_run(&repo.id, SHA_A))
        .await
        .expect("enqueue");
    let run_id = run.run().id.clone();

    let jobs = queue::create_jobs(
        db,
        &run_id,
        &[
            PlannedJob {
                stage: "build".to_string(),
                name: "cargo".to_string(),
                needs: vec![],
                status: Status::Queued,
                error: None,
            },
            PlannedJob {
                stage: "deploy".to_string(),
                name: "k8s".to_string(),
                needs: vec!["build".to_string()],
                status: Status::Skipped,
                error: Some("excluded by its `when` condition".to_string()),
            },
        ],
    )
    .await
    .expect("create jobs");

    (run_id, jobs[0].id.clone())
}

#[tokio::test]
async fn the_whole_plan_is_recorded_including_what_will_not_run() {
    // A run whose page grows as it goes can never show what it decided not to
    // do, which is the question a `when` condition generates.
    let Some((db, _guard)) = database().await else {
        return skipped("the_whole_plan_is_recorded_including_what_will_not_run");
    };
    let (run_id, _) = a_job(&db).await;

    let jobs = queue::list_jobs(&db, &run_id).await.expect("list jobs");
    assert_eq!(jobs.len(), 2);

    let deploy = jobs.iter().find(|j| j.stage == "deploy").expect("deploy");
    assert_eq!(deploy.status, Status::Skipped);
    assert_eq!(deploy.needs, ["build"]);
    assert!(deploy.error.as_deref().is_some_and(|e| e.contains("when")));
}

#[tokio::test]
async fn a_jobs_outcome_is_recorded() {
    let Some((db, _guard)) = database().await else {
        return skipped("a_jobs_outcome_is_recorded");
    };
    let (run_id, job_id) = a_job(&db).await;

    queue::start_job(&db, &job_id).await.expect("start");
    queue::finish_job(&db, &job_id, Status::Failed, Some(3), Some("it broke"))
        .await
        .expect("finish");

    let jobs = queue::list_jobs(&db, &run_id).await.expect("list");
    let build = jobs.iter().find(|j| j.stage == "build").expect("build");
    assert_eq!(build.status, Status::Failed);
    assert_eq!(build.exit_code, Some(3));
    assert!(build.started_at.is_some() && build.finished_at.is_some());
}

#[tokio::test]
async fn recording_steps_twice_replaces_rather_than_duplicates() {
    // A retried job would otherwise accumulate two sets of rows under the same
    // ordinals, and the unique index on (job_id, ordinal) would reject it.
    let Some((db, _guard)) = database().await else {
        return skipped("recording_steps_twice_replaces_rather_than_duplicates");
    };
    let (_, job_id) = a_job(&db).await;

    let steps = vec![StepState {
        ordinal: 0,
        kind: "run".to_string(),
        command: "cargo build".to_string(),
        status: Status::Success,
        exit_code: Some(0),
        started_at: Some(Utc::now()),
        finished_at: Some(Utc::now()),
    }];

    queue::record_steps(&db, &job_id, &steps)
        .await
        .expect("first");
    queue::record_steps(&db, &job_id, &steps)
        .await
        .expect("second");
}

#[tokio::test]
async fn logs_round_trip_in_order() {
    let Some((db, _guard)) = database().await else {
        return skipped("logs_round_trip_in_order");
    };
    let (_, job_id) = a_job(&db).await;

    let chunks: Vec<LogChunk> = (0..5)
        .map(|seq| LogChunk {
            seq,
            stream: if seq == 3 {
                Stream::Stderr
            } else {
                Stream::Stdout
            },
            line: format!("line {seq}"),
            at: Utc::now(),
        })
        .collect();

    queue::append_logs(&db, &job_id, &chunks)
        .await
        .expect("append");

    let read = queue::read_logs(&db, &job_id, -1).await.expect("read");
    assert_eq!(read.len(), 5);
    assert_eq!(read[0].line, "line 0");
    assert_eq!(read[4].line, "line 4");
    assert_eq!(read[3].stream, Stream::Stderr);
    assert!(read.windows(2).all(|w| w[0].seq < w[1].seq));
}

#[tokio::test]
async fn appending_the_same_logs_twice_is_harmless() {
    // Which is what makes a retried write safe after a partial failure.
    let Some((db, _guard)) = database().await else {
        return skipped("appending_the_same_logs_twice_is_harmless");
    };
    let (_, job_id) = a_job(&db).await;

    let chunks = vec![LogChunk {
        seq: 0,
        stream: Stream::Stdout,
        line: "once".to_string(),
        at: Utc::now(),
    }];

    queue::append_logs(&db, &job_id, &chunks)
        .await
        .expect("first");
    queue::append_logs(&db, &job_id, &chunks)
        .await
        .expect("second");

    assert_eq!(
        queue::read_logs(&db, &job_id, -1)
            .await
            .expect("read")
            .len(),
        1
    );
}

#[tokio::test]
async fn reading_logs_can_resume_from_a_sequence_number() {
    let Some((db, _guard)) = database().await else {
        return skipped("reading_logs_can_resume_from_a_sequence_number");
    };
    let (_, job_id) = a_job(&db).await;

    let chunks: Vec<LogChunk> = (0..4)
        .map(|seq| LogChunk {
            seq,
            stream: Stream::Stdout,
            line: format!("line {seq}"),
            at: Utc::now(),
        })
        .collect();
    queue::append_logs(&db, &job_id, &chunks)
        .await
        .expect("append");

    let rest = queue::read_logs(&db, &job_id, 1).await.expect("read");
    assert_eq!(rest.len(), 2);
    assert_eq!(rest[0].seq, 2);
}
