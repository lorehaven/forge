//! Run-result subscriptions and the outbox that carries them to gatehouse, against a real Postgres.

use crate::support::{database, register_repo, skipped};
use async_trait::async_trait;
use conveyor_service::domain::{Run, Status, Trigger};
use conveyor_service::notifications::gatehouse::{Delivery, Notifier};
use conveyor_service::notifications::outbox::{self, MAX_ATTEMPTS, Pending};
use conveyor_service::notifications::subscriptions::{self, Scope};
use conveyor_service::notifications::{
    Notifications, RUN_FAILED, RUN_RECOVERED, deliver_due, event_for, on_run_finished,
};
use conveyor_service::scheduler::projects::{self, NewProject};
use conveyor_service::scheduler::queue::{self, NewRun};
use quench_auth::prelude::UserDb;
use quench_db::prelude::{Database, Db};
use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::Duration;

const REF: &str = "refs/heads/master";

async fn user(db: &Db, name: &str, permissions: &str) {
    db.execute(&format!(
        "INSERT INTO auth.users (username, password, roles, permissions) \
         VALUES ('{name}', 'x', '[]'::jsonb, '{permissions}'::jsonb) \
         ON CONFLICT (username) DO UPDATE SET permissions = EXCLUDED.permissions, disabled_at = NULL"
    ))
    .await
    .expect("seed a user");
}

/// A run that has ended as `status`, queued after every earlier one.
async fn finished_run(db: &Db, repo_id: &str, git_ref: &str, status: Status) -> Run {
    tokio::time::sleep(Duration::from_millis(5)).await;
    let enqueued = queue::enqueue(
        db,
        &NewRun {
            repo_id: repo_id.to_string(),
            trigger: Trigger::Push,
            git_ref: git_ref.to_string(),
            sha: "a".repeat(40),
            message: None,
            delivery_id: None,
            resumed_from: None,
        },
    )
    .await
    .expect("enqueue");
    let run = enqueued.run().clone();
    queue::finish_run(db, &run.id, status, None)
        .await
        .expect("finish");
    queue::read_run(db, &run.id).await.unwrap().unwrap()
}

fn vars() -> BTreeMap<String, String> {
    [
        ("project", "p"),
        ("run", "r"),
        ("ref", "master"),
        ("url", "u"),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect()
}

// ---------------------------------------------------------------------------
// Subscriptions
// ---------------------------------------------------------------------------

#[tokio::test]
async fn following_is_idempotent_and_listed_and_can_be_undone() {
    let Some((db, _guard)) = database().await else {
        return skipped("following_is_idempotent_and_listed_and_can_be_undone");
    };
    let repo = register_repo(&db, "one", "file:///nowhere").await;

    let scope = Scope::Repo(&repo.id);
    assert!(
        subscriptions::subscribe(&db, "conveyor-tests", scope)
            .await
            .unwrap()
    );
    assert!(
        !subscriptions::subscribe(&db, "conveyor-tests", scope)
            .await
            .unwrap()
    );

    let project = Scope::Project(&repo.project_id);
    assert!(
        subscriptions::subscribe(&db, "conveyor-tests", project)
            .await
            .unwrap()
    );

    let mine = subscriptions::list_for_user(&db, "conveyor-tests")
        .await
        .unwrap();
    assert_eq!(mine.len(), 2);
    assert!(mine.iter().any(|s| s.repo_id.as_deref() == Some(&repo.id)));
    assert!(
        mine.iter()
            .any(|s| s.project_id.as_deref() == Some(&repo.project_id))
    );

    assert!(
        subscriptions::unsubscribe(&db, "conveyor-tests", scope)
            .await
            .unwrap()
    );
    assert!(
        !subscriptions::unsubscribe(&db, "conveyor-tests", scope)
            .await
            .unwrap()
    );
    assert_eq!(
        subscriptions::list_for_user(&db, "conveyor-tests")
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn deleting_a_repository_takes_its_subscriptions_with_it() {
    let Some((db, _guard)) = database().await else {
        return skipped("deleting_a_repository_takes_its_subscriptions_with_it");
    };
    let repo = register_repo(&db, "gone", "file:///nowhere").await;
    subscriptions::subscribe(&db, "conveyor-tests", Scope::Repo(&repo.id))
        .await
        .unwrap();

    conveyor_service::scheduler::repos::delete(&db, &repo.id)
        .await
        .unwrap();

    assert!(
        subscriptions::list_for_user(&db, "conveyor-tests")
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn a_repo_is_followed_directly_or_through_any_project_above_it_once_each() {
    let Some((db, _guard)) = database().await else {
        return skipped("a_repo_is_followed_directly_or_through_any_project_above_it_once_each");
    };
    let repo = register_repo(&db, "leaf", "file:///nowhere").await;
    // Nest the repo's project under a further parent, so "above" is more than one step.
    let parent = projects::create(
        &db,
        &NewProject {
            name: "parent".into(),
            parent_id: None,
        },
    )
    .await
    .unwrap();
    projects::move_to(&db, &repo.project_id, Some(&parent.id))
        .await
        .unwrap();
    let other = register_repo(&db, "elsewhere", "file:///nowhere").await;

    for name in ["alice", "bob", "carol", "dave"] {
        user(&db, name, "{}").await;
    }
    follow(&db, "alice", Scope::Repo(&repo.id)).await;
    follow(&db, "bob", Scope::Project(&repo.project_id)).await;
    follow(&db, "carol", Scope::Project(&parent.id)).await;
    // Alice also follows through the project - still one entry.
    follow(&db, "alice", Scope::Project(&parent.id)).await;
    // Dave follows a different repo entirely.
    follow(&db, "dave", Scope::Repo(&other.id)).await;

    let followers = subscriptions::subscribers(&db, &repo.id, &repo.project_id)
        .await
        .unwrap();
    assert_eq!(followers, vec!["alice", "bob", "carol"]);
}

// ---------------------------------------------------------------------------
// What a run's end calls for
// ---------------------------------------------------------------------------

#[tokio::test]
async fn failures_are_reported_and_so_is_the_first_success_after_one() {
    let Some((db, _guard)) = database().await else {
        return skipped("failures_are_reported_and_so_is_the_first_success_after_one");
    };
    let repo = register_repo(&db, "flaky", "file:///nowhere").await;

    // The very first run passing is not news.
    let first = finished_run(&db, &repo.id, REF, Status::Success).await;
    assert_eq!(event_for(&db, &first, Status::Success).await.unwrap(), None);

    let broken = finished_run(&db, &repo.id, REF, Status::Failed).await;
    assert_eq!(
        event_for(&db, &broken, Status::Failed).await.unwrap(),
        Some(RUN_FAILED)
    );

    // A cancelled run in between says nothing and does not hide the failure behind it.
    let cancelled = finished_run(&db, &repo.id, REF, Status::Cancelled).await;
    assert_eq!(
        event_for(&db, &cancelled, Status::Cancelled).await.unwrap(),
        None
    );

    let fixed = finished_run(&db, &repo.id, REF, Status::Success).await;
    assert_eq!(
        event_for(&db, &fixed, Status::Success).await.unwrap(),
        Some(RUN_RECOVERED)
    );

    // Green after green is quiet again.
    let steady = finished_run(&db, &repo.id, REF, Status::Success).await;
    assert_eq!(
        event_for(&db, &steady, Status::Success).await.unwrap(),
        None
    );
}

#[tokio::test]
async fn a_failure_on_another_ref_does_not_make_this_success_a_recovery() {
    let Some((db, _guard)) = database().await else {
        return skipped("a_failure_on_another_ref_does_not_make_this_success_a_recovery");
    };
    let repo = register_repo(&db, "refs", "file:///nowhere").await;
    finished_run(&db, &repo.id, "refs/heads/feature", Status::Failed).await;
    let main = finished_run(&db, &repo.id, REF, Status::Success).await;
    assert_eq!(event_for(&db, &main, Status::Success).await.unwrap(), None);
}

// ---------------------------------------------------------------------------
// Queueing for the followers of a finished run
// ---------------------------------------------------------------------------

async fn follow(db: &Db, name: &str, scope: Scope<'_>) {
    subscriptions::subscribe(db, name, scope).await.unwrap();
}

async fn due(db: &Db) -> Vec<Pending> {
    outbox::claim_due(db, 100).await.unwrap()
}

#[tokio::test]
async fn a_finished_run_is_queued_for_each_follower_who_can_still_read_it() {
    let Some((db, _guard)) = database().await else {
        return skipped("a_finished_run_is_queued_for_each_follower_who_can_still_read_it");
    };
    envmnt::set("CONVEYOR_PUBLIC_URL", "https://forge.example.test/conveyor");
    let repo = register_repo(&db, "watched", "file:///nowhere").await;

    let project_read = format!("{{\"conveyor\": [\"project:{}:read\"]}}", repo.project_id);
    user(&db, "reader", "{\"conveyor\": [\"read\"]}").await;
    user(&db, "scoped", &project_read).await;
    user(&db, "outsider", "{}").await;
    user(&db, "benched", "{\"conveyor\": [\"read\"]}").await;
    db.execute("UPDATE auth.users SET disabled_at = NOW() WHERE username = 'benched'")
        .await
        .unwrap();
    for name in [
        "reader",
        "scoped",
        "outsider",
        "benched",
        "ghost-not-in-realm",
    ] {
        // Not every one can be inserted: the realm row is what the foreign key wants.
        let _ = subscriptions::subscribe(&db, name, Scope::Repo(&repo.id)).await;
    }

    let notifications = Notifications {
        user_db: UserDb::init(db.clone()).await,
        auth_enabled: true,
    };
    let run = finished_run(&db, &repo.id, REF, Status::Failed).await;
    let queued = on_run_finished(&db, &notifications, &repo, &run, Status::Failed)
        .await
        .unwrap();

    assert_eq!(queued, 2);
    let mut sent = due(&db).await;
    sent.sort_by(|a, b| a.username.cmp(&b.username));
    assert_eq!(
        sent.iter().map(|m| m.username.as_str()).collect::<Vec<_>>(),
        vec!["reader", "scoped"]
    );
    let message = &sent[0];
    assert_eq!(message.template, RUN_FAILED);
    assert_eq!(message.run_id, run.id);
    assert_eq!(
        message.vars["url"],
        format!("https://forge.example.test/conveyor/ui/runs/{}", run.id)
    );
    assert_eq!(message.vars["ref"], "master");
    assert_eq!(message.vars["project"], "watched");
    for key in ["project", "run", "ref", "url"] {
        assert!(message.vars.contains_key(key), "{key}");
    }
    assert_eq!(
        message.vars.len(),
        4,
        "gatehouse accepts exactly these variables"
    );
}

#[tokio::test]
async fn finishing_twice_queues_once() {
    let Some((db, _guard)) = database().await else {
        return skipped("finishing_twice_queues_once");
    };
    envmnt::set("CONVEYOR_PUBLIC_URL", "https://forge.example.test/conveyor");
    let repo = register_repo(&db, "twice", "file:///nowhere").await;
    subscriptions::subscribe(&db, "conveyor-tests", Scope::Repo(&repo.id))
        .await
        .unwrap();
    let notifications = Notifications {
        user_db: UserDb::init(db.clone()).await,
        auth_enabled: false,
    };

    let run = finished_run(&db, &repo.id, REF, Status::Failed).await;
    assert_eq!(
        on_run_finished(&db, &notifications, &repo, &run, Status::Failed)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        on_run_finished(&db, &notifications, &repo, &run, Status::Failed)
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn nothing_is_queued_for_a_run_nobody_needs_to_hear_about() {
    let Some((db, _guard)) = database().await else {
        return skipped("nothing_is_queued_for_a_run_nobody_needs_to_hear_about");
    };
    envmnt::set("CONVEYOR_PUBLIC_URL", "https://forge.example.test/conveyor");
    let repo = register_repo(&db, "quiet", "file:///nowhere").await;
    subscriptions::subscribe(&db, "conveyor-tests", Scope::Repo(&repo.id))
        .await
        .unwrap();
    let notifications = Notifications {
        user_db: UserDb::init(db.clone()).await,
        auth_enabled: false,
    };

    let ok = finished_run(&db, &repo.id, REF, Status::Success).await;
    assert_eq!(
        on_run_finished(&db, &notifications, &repo, &ok, Status::Success)
            .await
            .unwrap(),
        0
    );
    let stopped = finished_run(&db, &repo.id, REF, Status::Cancelled).await;
    assert_eq!(
        on_run_finished(&db, &notifications, &repo, &stopped, Status::Cancelled)
            .await
            .unwrap(),
        0
    );
    assert!(due(&db).await.is_empty());
}

// ---------------------------------------------------------------------------
// The outbox
// ---------------------------------------------------------------------------

/// Answers with whatever it is told and remembers what it was asked.
struct Scripted {
    answer: Mutex<Delivery>,
    seen: Mutex<Vec<Pending>>,
}

impl Scripted {
    fn new(answer: Delivery) -> Self {
        Self {
            answer: Mutex::new(answer),
            seen: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl Notifier for Scripted {
    async fn deliver(&self, message: &Pending) -> Delivery {
        self.seen.lock().unwrap().push(message.clone());
        self.answer.lock().unwrap().clone()
    }
}

async fn one_queued(db: &Db, name: &str) -> String {
    let repo = register_repo(db, name, "file:///nowhere").await;
    let run = finished_run(db, &repo.id, REF, Status::Failed).await;
    assert!(
        outbox::enqueue(db, &run.id, "conveyor-tests", RUN_FAILED, &vars())
            .await
            .unwrap()
    );
    run.id
}

async fn row(db: &Db, run_id: &str) -> Option<(i32, Option<String>, bool)> {
    let pool = queue::pool(db).unwrap();
    sqlx::query_as::<_, (i32, Option<String>, bool)>(
        "SELECT attempts, last_error, failed_at IS NOT NULL \
         FROM conveyor.notification_outbox WHERE run_id = $1",
    )
    .bind(run_id)
    .fetch_optional(pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn a_claimed_message_is_leased_so_a_second_sender_skips_it() {
    let Some((db, _guard)) = database().await else {
        return skipped("a_claimed_message_is_leased_so_a_second_sender_skips_it");
    };
    one_queued(&db, "leased").await;

    let first = outbox::claim_due(&db, 10).await.unwrap();
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].attempts, 1);
    assert!(outbox::claim_due(&db, 10).await.unwrap().is_empty());
}

#[tokio::test]
async fn a_message_gatehouse_took_is_removed() {
    let Some((db, _guard)) = database().await else {
        return skipped("a_message_gatehouse_took_is_removed");
    };
    let run_id = one_queued(&db, "done").await;
    let notifier = Scripted::new(Delivery::Done);

    assert_eq!(deliver_due(&db, &notifier).await.unwrap(), 1);

    assert!(row(&db, &run_id).await.is_none());
    let seen = notifier.seen.lock().unwrap();
    assert_eq!(seen[0].run_id, run_id);
    assert_eq!(seen[0].template, RUN_FAILED);
}

#[tokio::test]
async fn a_message_that_cannot_be_delivered_yet_is_kept_and_tried_later() {
    let Some((db, _guard)) = database().await else {
        return skipped("a_message_that_cannot_be_delivered_yet_is_kept_and_tried_later");
    };
    let run_id = one_queued(&db, "retry").await;
    let notifier = Scripted::new(Delivery::Retry("gatehouse is down".into()));

    deliver_due(&db, &notifier).await.unwrap();

    let (attempts, error, failed) = row(&db, &run_id).await.expect("still queued");
    assert_eq!(attempts, 1);
    assert_eq!(error.as_deref(), Some("gatehouse is down"));
    assert!(!failed);
    // Backed off: not due again straight away.
    assert_eq!(deliver_due(&db, &notifier).await.unwrap(), 0);
}

#[tokio::test]
async fn a_message_gatehouse_refuses_is_set_aside_not_retried() {
    let Some((db, _guard)) = database().await else {
        return skipped("a_message_gatehouse_refuses_is_set_aside_not_retried");
    };
    let run_id = one_queued(&db, "rejected").await;
    let notifier = Scripted::new(Delivery::Rejected("HTTP 400: bad variable".into()));

    deliver_due(&db, &notifier).await.unwrap();

    let (_, error, failed) = row(&db, &run_id).await.expect("kept for a human");
    assert!(failed);
    assert_eq!(error.as_deref(), Some("HTTP 400: bad variable"));
    assert!(outbox::claim_due(&db, 10).await.unwrap().is_empty());
}

#[tokio::test]
async fn a_message_that_keeps_failing_is_eventually_given_up_on() {
    let Some((db, _guard)) = database().await else {
        return skipped("a_message_that_keeps_failing_is_eventually_given_up_on");
    };
    let run_id = one_queued(&db, "hopeless").await;
    let notifier = Scripted::new(Delivery::Retry("still down".into()));

    for _ in 0..MAX_ATTEMPTS {
        // Make it due again without waiting out the backoff.
        db.execute("UPDATE conveyor.notification_outbox SET next_attempt_at = NOW() WHERE failed_at IS NULL")
            .await
            .unwrap();
        deliver_due(&db, &notifier).await.unwrap();
    }

    let (attempts, _, failed) = row(&db, &run_id).await.unwrap();
    assert_eq!(attempts, MAX_ATTEMPTS);
    assert!(failed);
    assert_eq!(notifier.seen.lock().unwrap().len() as i32, MAX_ATTEMPTS);
}

#[tokio::test]
async fn a_message_left_leased_by_a_crashed_sender_is_picked_up_again() {
    let Some((db, _guard)) = database().await else {
        return skipped("a_message_left_leased_by_a_crashed_sender_is_picked_up_again");
    };
    let run_id = one_queued(&db, "crashed").await;
    outbox::claim_due(&db, 10).await.unwrap();
    // The lease runs out.
    db.execute(
        "UPDATE conveyor.notification_outbox SET next_attempt_at = NOW() - interval '1 second'",
    )
    .await
    .unwrap();

    let again = outbox::claim_due(&db, 10).await.unwrap();
    assert_eq!(again.len(), 1);
    assert_eq!(again[0].run_id, run_id);
    assert_eq!(again[0].attempts, 2);
}
