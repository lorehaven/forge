//! The webhook endpoint, from a signed delivery to a queued run.
//!
//! The unit tests cover signatures and payload parsing on their own. These
//! cover the order the endpoint does things in, which is where the security
//! properties live: verify before parsing, refuse an unregistered repository,
//! refuse a fork, and queue exactly once per delivery.

use crate::support::{database, register_repo, skipped};
use actix_web::http::StatusCode;
use actix_web::{App, test, web};
use conveyor_service::config::ConveyorConfig;
use conveyor_service::domain::{Status, Trigger};
use conveyor_service::providers::{Providers, sign_sha256};
use conveyor_service::routers::api;
use conveyor_service::scheduler::queue;
use quench_auth::prelude::JwtConfig;
use quench_db::prelude::Db;
use serde_json::json;

const SECRET: &str = "conveyor-webhook-tests";

/// Sets the environment the endpoint reads. Called under the database guard,
/// so the tests do not race over it.
fn configure(secret: Option<&str>) {
    unsafe { std::env::set_var("SERVICE_AUTH_ENABLED", "false") };
    match secret {
        Some(secret) => unsafe { std::env::set_var("CONVEYOR_WEBHOOK_SECRET", secret) },
        None => unsafe { std::env::remove_var("CONVEYOR_WEBHOOK_SECRET") },
    }
}

/// Posts a delivery, signed with `signing_secret`, and returns the status.
async fn deliver(
    db: &Db,
    config: ConveyorConfig,
    event: &str,
    body: &serde_json::Value,
    signing_secret: &str,
    delivery_id: &str,
) -> StatusCode {
    let raw = body.to_string();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.clone()))
            .app_data(web::Data::new(Providers::from_env()))
            .app_data(web::Data::new(config))
            .service(api::scope(JwtConfig::for_tests())),
    )
    .await;

    let request = test::TestRequest::post()
        .uri("/api/v1/webhooks/github")
        .insert_header(("x-github-event", event))
        .insert_header(("x-github-delivery", delivery_id))
        .insert_header((
            "x-hub-signature-256",
            sign_sha256(raw.as_bytes(), signing_secret.as_bytes()),
        ))
        .insert_header(("content-type", "application/json"))
        .set_payload(raw)
        .to_request();

    test::call_service(&app, request).await.status()
}

fn push(owner: &str, name: &str, git_ref: &str, sha: &str) -> serde_json::Value {
    json!({
        "ref": git_ref,
        "after": sha,
        "deleted": false,
        "head_commit": { "message": "a commit" },
        "repository": {
            "name": name,
            "full_name": format!("{owner}/{name}"),
            "owner": { "login": owner }
        }
    })
}

fn pull_request(owner: &str, name: &str, head_repo: &str) -> serde_json::Value {
    json!({
        "action": "opened",
        "number": 7,
        "pull_request": {
            "title": "a change",
            "head": { "ref": "topic", "sha": "b".repeat(40), "repo": { "full_name": head_repo } }
        },
        "repository": {
            "name": name,
            "full_name": format!("{owner}/{name}"),
            "owner": { "login": owner }
        }
    })
}

async fn run_count(db: &Db) -> usize {
    queue::list_runs(db, None, 500).await.expect("list").len()
}

/// A registered GitHub repository, named as the payloads name it.
async fn register_github(db: &Db, owner: &str, name: &str) -> conveyor_service::domain::Repo {
    let repo = register_repo(db, name, "file:///nowhere").await;
    // `register_repo` registers a `generic` one; the webhook looks a repository
    // up by provider as well as by slug.
    let schema = queue::schema();
    quench_db::prelude::Database::execute(
        db,
        &format!(
            "UPDATE {schema}.repos SET provider = 'github', owner = '{owner}', name = '{name}' \
             WHERE id = '{}'",
            repo.id
        ),
    )
    .await
    .expect("make it a github repository");

    conveyor_service::scheduler::repos::read(db, &repo.id)
        .await
        .expect("read")
        .expect("still there")
}

// ---------------------------------------------------------------------------

#[actix_web::test]
async fn a_signed_push_queues_a_run() {
    let Some((db, _guard)) = database().await else {
        return skipped("a_signed_push_queues_a_run");
    };
    configure(Some(SECRET));
    let repo = register_github(&db, "lorehaven", "forge").await;

    let sha = "a".repeat(40);
    let status = deliver(
        &db,
        ConveyorConfig::default(),
        "push",
        &push("lorehaven", "forge", "refs/heads/master", &sha),
        SECRET,
        "d-1",
    )
    .await;

    assert_eq!(status, StatusCode::ACCEPTED);

    let runs = queue::list_runs(&db, Some(&repo.id), 10)
        .await
        .expect("list");
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].status, Status::Queued);
    assert_eq!(runs[0].trigger, Trigger::Push);
    assert_eq!(runs[0].sha, sha);
    assert_eq!(runs[0].delivery_id.as_deref(), Some("d-1"));
    assert_eq!(runs[0].message.as_deref(), Some("a commit"));
}

#[actix_web::test]
async fn a_bad_signature_is_rejected_and_queues_nothing() {
    let Some((db, _guard)) = database().await else {
        return skipped("a_bad_signature_is_rejected_and_queues_nothing");
    };
    configure(Some(SECRET));
    register_github(&db, "lorehaven", "forge").await;

    let status = deliver(
        &db,
        ConveyorConfig::default(),
        "push",
        &push("lorehaven", "forge", "refs/heads/master", &"a".repeat(40)),
        "not-the-secret",
        "d-1",
    )
    .await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(
        run_count(&db).await,
        0,
        "an unverified delivery must queue nothing"
    );
}

#[actix_web::test]
async fn a_redelivery_does_not_queue_a_second_run() {
    // A provider retries a delivery it did not get a prompt answer for, and a
    // second run of the same commit would double every side effect.
    let Some((db, _guard)) = database().await else {
        return skipped("a_redelivery_does_not_queue_a_second_run");
    };
    configure(Some(SECRET));
    register_github(&db, "lorehaven", "forge").await;

    let body = push("lorehaven", "forge", "refs/heads/master", &"a".repeat(40));
    let first = deliver(&db, ConveyorConfig::default(), "push", &body, SECRET, "d-1").await;
    let second = deliver(&db, ConveyorConfig::default(), "push", &body, SECRET, "d-1").await;

    assert_eq!(first, StatusCode::ACCEPTED);
    assert_eq!(second, StatusCode::OK, "the retry should be told it landed");
    assert_eq!(run_count(&db).await, 1);
}

#[actix_web::test]
async fn a_delivery_for_an_unregistered_repository_is_refused() {
    // Registration is explicit on purpose: conveyor runs code the repository
    // supplies, so a delivery is not an invitation to start building it.
    let Some((db, _guard)) = database().await else {
        return skipped("a_delivery_for_an_unregistered_repository_is_refused");
    };
    configure(Some(SECRET));

    let status = deliver(
        &db,
        ConveyorConfig::default(),
        "push",
        &push("someone", "unknown", "refs/heads/master", &"a".repeat(40)),
        SECRET,
        "d-1",
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(run_count(&db).await, 0);
}

#[actix_web::test]
async fn a_delivery_for_a_disabled_repository_is_accepted_and_ignored() {
    // Accepted rather than refused: a provider retries a 4xx, and there is
    // nothing here for a retry to fix.
    let Some((db, _guard)) = database().await else {
        return skipped("a_delivery_for_a_disabled_repository_is_accepted_and_ignored");
    };
    configure(Some(SECRET));
    let repo = register_github(&db, "lorehaven", "forge").await;
    conveyor_service::scheduler::repos::set_enabled(&db, &repo.id, false)
        .await
        .expect("disable");

    let status = deliver(
        &db,
        ConveyorConfig::default(),
        "push",
        &push("lorehaven", "forge", "refs/heads/master", &"a".repeat(40)),
        SECRET,
        "d-1",
    )
    .await;

    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(run_count(&db).await, 0);
}

#[actix_web::test]
async fn a_pull_request_from_a_fork_is_not_built_by_default() {
    // Its pipeline is written by someone outside the estate, and under the
    // native executor it would run with this service's privileges.
    let Some((db, _guard)) = database().await else {
        return skipped("a_pull_request_from_a_fork_is_not_built_by_default");
    };
    configure(Some(SECRET));
    register_github(&db, "lorehaven", "forge").await;

    let status = deliver(
        &db,
        ConveyorConfig::default(),
        "pull_request",
        &pull_request("lorehaven", "forge", "someone-else/forge"),
        SECRET,
        "d-1",
    )
    .await;

    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(run_count(&db).await, 0, "a fork must not build by default");
}

#[actix_web::test]
async fn a_pull_request_from_a_fork_builds_when_the_deployment_allows_it() {
    let Some((db, _guard)) = database().await else {
        return skipped("a_pull_request_from_a_fork_builds_when_the_deployment_allows_it");
    };
    configure(Some(SECRET));
    register_github(&db, "lorehaven", "forge").await;

    let config = ConveyorConfig {
        allow_fork_pr: true,
        ..ConveyorConfig::default()
    };
    let status = deliver(
        &db,
        config,
        "pull_request",
        &pull_request("lorehaven", "forge", "someone-else/forge"),
        SECRET,
        "d-1",
    )
    .await;

    assert_eq!(status, StatusCode::ACCEPTED);

    let runs = queue::list_runs(&db, None, 10).await.expect("list");
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].trigger, Trigger::PullRequest);
    // A fork's branch does not exist in the base repository.
    assert_eq!(runs[0].git_ref, "refs/pull/7/head");
}

#[actix_web::test]
async fn a_same_repository_pull_request_builds_its_branch() {
    let Some((db, _guard)) = database().await else {
        return skipped("a_same_repository_pull_request_builds_its_branch");
    };
    configure(Some(SECRET));
    register_github(&db, "lorehaven", "forge").await;

    deliver(
        &db,
        ConveyorConfig::default(),
        "pull_request",
        &pull_request("lorehaven", "forge", "lorehaven/forge"),
        SECRET,
        "d-1",
    )
    .await;

    let runs = queue::list_runs(&db, None, 10).await.expect("list");
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].git_ref, "refs/heads/topic");
}

#[actix_web::test]
async fn a_ping_is_accepted_and_queues_nothing() {
    let Some((db, _guard)) = database().await else {
        return skipped("a_ping_is_accepted_and_queues_nothing");
    };
    configure(Some(SECRET));
    register_github(&db, "lorehaven", "forge").await;

    let status = deliver(
        &db,
        ConveyorConfig::default(),
        "ping",
        &json!({ "zen": "Design for failure." }),
        SECRET,
        "d-1",
    )
    .await;

    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(run_count(&db).await, 0);
}

#[actix_web::test]
async fn a_hostile_ref_is_refused_before_it_reaches_the_queue() {
    // `git` reads a leading `-` as an option, and `--upload-pack=...` where a
    // ref was expected runs a program of the sender's choosing. The ref comes
    // from a body somebody else wrote.
    let Some((db, _guard)) = database().await else {
        return skipped("a_hostile_ref_is_refused_before_it_reaches_the_queue");
    };
    configure(Some(SECRET));
    register_github(&db, "lorehaven", "forge").await;

    let status = deliver(
        &db,
        ConveyorConfig::default(),
        "push",
        &push(
            "lorehaven",
            "forge",
            "--upload-pack=/tmp/pwned",
            &"a".repeat(40),
        ),
        SECRET,
        "d-1",
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(run_count(&db).await, 0);
}

#[actix_web::test]
async fn a_sha_that_is_not_a_sha_is_refused() {
    let Some((db, _guard)) = database().await else {
        return skipped("a_sha_that_is_not_a_sha_is_refused");
    };
    configure(Some(SECRET));
    register_github(&db, "lorehaven", "forge").await;

    let status = deliver(
        &db,
        ConveyorConfig::default(),
        "push",
        &push("lorehaven", "forge", "refs/heads/master", "; rm -rf /"),
        SECRET,
        "d-1",
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(run_count(&db).await, 0);
}

#[actix_web::test]
async fn without_a_configured_secret_the_endpoint_refuses_to_serve() {
    // Every delivery would be unverified, which would let anyone on the network
    // start a build.
    let Some((db, _guard)) = database().await else {
        return skipped("without_a_configured_secret_the_endpoint_refuses_to_serve");
    };
    configure(None);
    register_github(&db, "lorehaven", "forge").await;

    let status = deliver(
        &db,
        ConveyorConfig::default(),
        "push",
        &push("lorehaven", "forge", "refs/heads/master", &"a".repeat(40)),
        SECRET,
        "d-1",
    )
    .await;

    // Put it back for whichever test runs next.
    configure(Some(SECRET));

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(run_count(&db).await, 0);
}
