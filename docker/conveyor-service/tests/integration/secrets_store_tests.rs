//! The secret store, against a real Postgres, and secrets reaching a real job.

use crate::support::{Origin, TEST_USER, database, register_repo, skipped};
use conveyor_service::config::ConveyorConfig;
use conveyor_service::domain::{Status, Trigger};
use conveyor_service::executors::NativeExecutor;
use conveyor_service::providers::Providers;
use conveyor_service::scheduler::queue::{self, NewRun};
use conveyor_service::scheduler::spawn_pool;
use conveyor_service::secrets::SecretKey;
use conveyor_service::secrets::store::{self, Scope, SecretError};
use quench_db::prelude::Db;
use std::sync::Arc;
use std::time::Duration;

const HEX_KEY: &str = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";

fn key() -> SecretKey {
    SecretKey::parse("CONVEYOR_SECRET_KEY", HEX_KEY).expect("key")
}

/// The key the worker reads from the environment. Set under the database
/// guard, so the tests do not race over it.
fn configure_key() {
    unsafe { std::env::set_var("CONVEYOR_SECRET_KEY", HEX_KEY) };
}

// ---------------------------------------------------------------------------
// The store
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_secret_round_trips_through_the_database() {
    let Some((db, _guard)) = database().await else {
        return skipped("a_secret_round_trips_through_the_database");
    };
    let repo = register_repo(&db, "one", "file:///nowhere").await;
    let scope = Scope::Repo(repo.id.clone());

    store::put(&db, &key(), &scope, "TOKEN", "a-real-token", TEST_USER)
        .await
        .expect("put");

    assert_eq!(
        store::get(&db, &key(), &scope, "TOKEN").await.expect("get"),
        Some("a-real-token".to_string())
    );
}

#[tokio::test]
async fn the_value_is_not_readable_from_the_table() {
    // The whole point of the store: somebody with the database has ciphertext,
    // not tokens.
    let Some((db, _guard)) = database().await else {
        return skipped("the_value_is_not_readable_from_the_table");
    };
    store::put(
        &db,
        &key(),
        &Scope::Global,
        "TOKEN",
        "a-recognisable-token",
        TEST_USER,
    )
    .await
    .expect("put");

    let schema = queue::schema();
    let pool = queue::pool(&db).expect("pool");
    let rows: Vec<(Vec<u8>,)> = sqlx::query_as(sqlx::AssertSqlSafe(
        format!("SELECT ciphertext FROM {schema}.secrets").as_str(),
    ))
    .fetch_all(pool)
    .await
    .expect("read the raw column");

    assert_eq!(rows.len(), 1);
    let stored = String::from_utf8_lossy(&rows[0].0);
    assert!(
        !stored.contains("a-recognisable-token"),
        "the plaintext is in the table"
    );
}

#[tokio::test]
async fn writing_the_same_name_replaces_the_value() {
    let Some((db, _guard)) = database().await else {
        return skipped("writing_the_same_name_replaces_the_value");
    };
    let scope = Scope::Global;

    store::put(&db, &key(), &scope, "TOKEN", "first-value", TEST_USER)
        .await
        .expect("put");
    store::put(&db, &key(), &scope, "TOKEN", "second-value", TEST_USER)
        .await
        .expect("put again");

    assert_eq!(
        store::get(&db, &key(), &scope, "TOKEN").await.expect("get"),
        Some("second-value".to_string())
    );
    assert_eq!(store::list(&db, &scope).await.expect("list").len(), 1);
}

#[tokio::test]
async fn a_repository_secret_and_a_global_one_can_share_a_name() {
    let Some((db, _guard)) = database().await else {
        return skipped("a_repository_secret_and_a_global_one_can_share_a_name");
    };
    let repo = register_repo(&db, "one", "file:///nowhere").await;
    let repo_scope = Scope::Repo(repo.id.clone());

    store::put(
        &db,
        &key(),
        &Scope::Global,
        "TOKEN",
        "the-estate-value",
        TEST_USER,
    )
    .await
    .expect("put global");
    store::put(
        &db,
        &key(),
        &repo_scope,
        "TOKEN",
        "the-repo-value",
        TEST_USER,
    )
    .await
    .expect("put repo");

    assert_eq!(
        store::get(&db, &key(), &Scope::Global, "TOKEN")
            .await
            .unwrap(),
        Some("the-estate-value".to_string())
    );
    assert_eq!(
        store::get(&db, &key(), &repo_scope, "TOKEN").await.unwrap(),
        Some("the-repo-value".to_string())
    );
}

#[tokio::test]
async fn a_repositorys_own_value_wins_over_the_estates() {
    // So a shared default can be set once and overridden where it matters.
    let Some((db, _guard)) = database().await else {
        return skipped("a_repositorys_own_value_wins_over_the_estates");
    };
    configure_key();
    let repo = register_repo(&db, "one", "file:///nowhere").await;

    store::put(
        &db,
        &key(),
        &Scope::Global,
        "TOKEN",
        "the-estate-value",
        TEST_USER,
    )
    .await
    .expect("put");
    store::put(
        &db,
        &key(),
        &Scope::Repo(repo.id.clone()),
        "TOKEN",
        "the-repo-value",
        TEST_USER,
    )
    .await
    .expect("put");

    let resolved = store::resolve(&db, Some(&key()), &repo.id, &["TOKEN".to_string()])
        .await
        .expect("resolve");

    assert_eq!(
        resolved.get("TOKEN").map(String::as_str),
        Some("the-repo-value")
    );
}

#[tokio::test]
async fn a_global_secret_is_visible_to_a_repository_that_has_none() {
    let Some((db, _guard)) = database().await else {
        return skipped("a_global_secret_is_visible_to_a_repository_that_has_none");
    };
    let repo = register_repo(&db, "one", "file:///nowhere").await;
    store::put(
        &db,
        &key(),
        &Scope::Global,
        "SHARED",
        "shared-value",
        TEST_USER,
    )
    .await
    .expect("put");

    let resolved = store::resolve(&db, Some(&key()), &repo.id, &["SHARED".to_string()])
        .await
        .expect("resolve");
    assert_eq!(
        resolved.get("SHARED").map(String::as_str),
        Some("shared-value")
    );
}

#[tokio::test]
async fn one_repositorys_secret_is_invisible_to_another() {
    let Some((db, _guard)) = database().await else {
        return skipped("one_repositorys_secret_is_invisible_to_another");
    };
    let alpha = register_repo(&db, "alpha", "file:///nowhere").await;
    let beta = register_repo(&db, "beta", "file:///nowhere").await;

    store::put(
        &db,
        &key(),
        &Scope::Repo(alpha.id.clone()),
        "TOKEN",
        "alphas-token",
        TEST_USER,
    )
    .await
    .expect("put");

    let error = store::resolve(&db, Some(&key()), &beta.id, &["TOKEN".to_string()])
        .await
        .expect_err("beta should not see it");
    assert!(matches!(error, SecretError::Missing { .. }), "{error:?}");
}

#[tokio::test]
async fn resolving_a_secret_nobody_set_is_an_error() {
    // Better than a blank token, which fails somewhere further on in a way that
    // takes much longer to understand.
    let Some((db, _guard)) = database().await else {
        return skipped("resolving_a_secret_nobody_set_is_an_error");
    };
    let repo = register_repo(&db, "one", "file:///nowhere").await;

    let error = store::resolve(&db, Some(&key()), &repo.id, &["NOPE".to_string()])
        .await
        .expect_err("should not resolve");

    let message = error.to_string();
    assert!(message.contains("NOPE"), "{message}");
}

#[tokio::test]
async fn resolving_nothing_needs_no_key() {
    // A pipeline that declares no secrets builds on a deployment that has never
    // set CONVEYOR_SECRET_KEY.
    let Some((db, _guard)) = database().await else {
        return skipped("resolving_nothing_needs_no_key");
    };
    let repo = register_repo(&db, "one", "file:///nowhere").await;

    assert!(
        store::resolve(&db, None, &repo.id, &[])
            .await
            .expect("resolve")
            .is_empty()
    );
}

#[tokio::test]
async fn resolving_a_secret_without_a_key_says_so() {
    let Some((db, _guard)) = database().await else {
        return skipped("resolving_a_secret_without_a_key_says_so");
    };
    let repo = register_repo(&db, "one", "file:///nowhere").await;

    let error = store::resolve(&db, None, &repo.id, &["TOKEN".to_string()])
        .await
        .expect_err("should not resolve");
    assert!(error.to_string().contains("CONVEYOR_SECRET_KEY"), "{error}");
}

#[tokio::test]
async fn listing_returns_names_and_never_values() {
    let Some((db, _guard)) = database().await else {
        return skipped("listing_returns_names_and_never_values");
    };
    store::put(
        &db,
        &key(),
        &Scope::Global,
        "TOKEN",
        "a-secret-value",
        TEST_USER,
    )
    .await
    .expect("put");

    let listed = store::list(&db, &Scope::Global).await.expect("list");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].name, "TOKEN");
    assert_eq!(listed[0].created_by, TEST_USER);

    // `SecretRef` has no value field at all; this asserts the serialised form
    // an API response is built from carries none either.
    let rendered = serde_json::to_string(&listed[0]).expect("serialise");
    assert!(!rendered.contains("a-secret-value"), "{rendered}");
}

#[tokio::test]
async fn deleting_removes_it() {
    let Some((db, _guard)) = database().await else {
        return skipped("deleting_removes_it");
    };
    store::put(&db, &key(), &Scope::Global, "TOKEN", "a-value", TEST_USER)
        .await
        .expect("put");

    assert!(
        store::delete(&db, &Scope::Global, "TOKEN")
            .await
            .expect("delete")
    );
    assert!(
        !store::delete(&db, &Scope::Global, "TOKEN")
            .await
            .expect("again")
    );
    assert!(
        store::get(&db, &key(), &Scope::Global, "TOKEN")
            .await
            .expect("get")
            .is_none()
    );
}

#[tokio::test]
async fn a_value_too_short_to_redact_is_refused() {
    let Some((db, _guard)) = database().await else {
        return skipped("a_value_too_short_to_redact_is_refused");
    };
    let error = store::put(&db, &key(), &Scope::Global, "TOKEN", "ab", TEST_USER)
        .await
        .expect_err("should be refused");
    assert!(matches!(error, SecretError::TooShort), "{error:?}");
}

#[tokio::test]
async fn a_name_that_is_not_an_environment_variable_is_refused() {
    let Some((db, _guard)) = database().await else {
        return skipped("a_name_that_is_not_an_environment_variable_is_refused");
    };
    for bad in ["", "with space", "with-dash", "1LEADING", "semi;colon"] {
        let error = store::put(&db, &key(), &Scope::Global, bad, "a-value", TEST_USER)
            .await
            .expect_err("should be refused");
        assert!(matches!(error, SecretError::BadName { .. }), "{bad:?}");
    }
}

#[tokio::test]
async fn deleting_a_repository_takes_its_secrets_with_it() {
    let Some((db, _guard)) = database().await else {
        return skipped("deleting_a_repository_takes_its_secrets_with_it");
    };
    let repo = register_repo(&db, "one", "file:///nowhere").await;
    let scope = Scope::Repo(repo.id.clone());

    store::put(&db, &key(), &scope, "TOKEN", "a-value", TEST_USER)
        .await
        .expect("put");
    conveyor_service::scheduler::repos::delete(&db, &repo.id)
        .await
        .expect("delete the repository");

    assert!(store::list(&db, &scope).await.expect("list").is_empty());
}

// ---------------------------------------------------------------------------
// Secrets reaching a job
// ---------------------------------------------------------------------------

fn start_scheduler(db: &Db) -> tempfile::TempDir {
    let work = tempfile::tempdir().expect("temp dir");
    spawn_pool(
        db.clone(),
        ConveyorConfig {
            work_dir: work.path().to_path_buf(),
            max_concurrent_runs: 1,
            default_job_timeout_secs: 60,
            checkout_timeout_secs: 60,
            ..ConveyorConfig::default()
        },
        Arc::new(NativeExecutor::new()),
        Arc::new(Providers::from_env()),
    );
    work
}

async fn settle(db: &Db, run_id: &str) -> conveyor_service::domain::Run {
    for _ in 0..600 {
        let run = queue::read_run(db, run_id)
            .await
            .expect("read")
            .expect("exists");
        if run.status.is_terminal() {
            return run;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("the run never finished");
}

async fn queue_run(db: &Db, repo_id: &str, sha: &str) -> String {
    queue::enqueue(
        db,
        &NewRun {
            repo_id: repo_id.to_string(),
            trigger: Trigger::Push,
            git_ref: "refs/heads/master".to_string(),
            sha: sha.to_string(),
            message: None,
            delivery_id: None,
        },
    )
    .await
    .expect("enqueue")
    .run()
    .id
    .clone()
}

const PIPELINE_USING_A_SECRET: &str = r#"
[[stage]]
name = "build"
[[stage.job]]
secrets = ["DEPLOY_TOKEN"]
steps = ["echo using $DEPLOY_TOKEN"]
"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_declared_secret_reaches_the_step_and_stays_out_of_the_log() {
    let Some((db, _guard)) = database().await else {
        return skipped("a_declared_secret_reaches_the_step_and_stays_out_of_the_log");
    };
    configure_key();

    let origin = Origin::with_pipeline(PIPELINE_USING_A_SECRET);
    let repo = register_repo(&db, "secrets", &origin.url()).await;
    store::put(
        &db,
        &key(),
        &Scope::Repo(repo.id.clone()),
        "DEPLOY_TOKEN",
        "s3cr3t-deploy-token",
        TEST_USER,
    )
    .await
    .expect("put");

    let _work = start_scheduler(&db);
    let run_id = queue_run(&db, &repo.id, &origin.sha).await;
    let run = settle(&db, &run_id).await;

    assert_eq!(run.status, Status::Success, "error was {:?}", run.error);

    let jobs = queue::list_jobs(&db, &run_id).await.expect("jobs");
    let logs = queue::read_logs(&db, &jobs[0].id, -1).await.expect("logs");
    let text = logs
        .iter()
        .map(|chunk| chunk.line.clone())
        .collect::<Vec<_>>()
        .join("\n");

    // The step ran and had the value - `using` came from the echo.
    assert!(text.contains("using"), "the step did not run: {text}");
    // And the value itself never reached the database.
    assert!(
        !text.contains("s3cr3t-deploy-token"),
        "the secret leaked into the stored log: {text}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_job_declaring_a_secret_nobody_set_fails_with_a_clear_reason() {
    let Some((db, _guard)) = database().await else {
        return skipped("a_job_declaring_a_secret_nobody_set_fails_with_a_clear_reason");
    };
    configure_key();

    let origin = Origin::with_pipeline(PIPELINE_USING_A_SECRET);
    let repo = register_repo(&db, "secrets", &origin.url()).await;

    let _work = start_scheduler(&db);
    let run_id = queue_run(&db, &repo.id, &origin.sha).await;
    let run = settle(&db, &run_id).await;

    assert_eq!(run.status, Status::Failed);

    let jobs = queue::list_jobs(&db, &run_id).await.expect("jobs");
    assert!(
        jobs[0]
            .error
            .as_deref()
            .is_some_and(|e| e.contains("DEPLOY_TOKEN")),
        "the job should name what was missing: {:?}",
        jobs[0].error
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_job_sees_only_the_secrets_it_named() {
    // The whole access model: everything else in the store is invisible to it.
    let Some((db, _guard)) = database().await else {
        return skipped("a_job_sees_only_the_secrets_it_named");
    };
    configure_key();

    let origin = Origin::with_pipeline(
        r#"
[[stage]]
name = "build"
[[stage.job]]
secrets = ["WANTED"]
steps = ["echo [$WANTED] [$UNWANTED]"]
"#,
    );
    let repo = register_repo(&db, "secrets", &origin.url()).await;
    let scope = Scope::Repo(repo.id.clone());

    store::put(&db, &key(), &scope, "WANTED", "wanted-value", TEST_USER)
        .await
        .expect("put");
    store::put(&db, &key(), &scope, "UNWANTED", "unwanted-value", TEST_USER)
        .await
        .expect("put");

    let _work = start_scheduler(&db);
    let run_id = queue_run(&db, &repo.id, &origin.sha).await;
    settle(&db, &run_id).await;

    let jobs = queue::list_jobs(&db, &run_id).await.expect("jobs");
    let text = queue::read_logs(&db, &jobs[0].id, -1)
        .await
        .expect("logs")
        .iter()
        .map(|chunk| chunk.line.clone())
        .collect::<Vec<_>>()
        .join("\n");

    // The undeclared one was never in the environment, so the shell expanded it
    // to nothing - and it is certainly not in the log.
    assert!(text.contains("[] ") || text.contains(" []"), "{text}");
    assert!(!text.contains("unwanted-value"), "{text}");
}
