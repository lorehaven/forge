//! Unit tests for `providers/generic.rs`, and for the parts of
//! `providers/mod.rs` that are not GitHub's.

use actix_web::http::header::{HeaderMap, HeaderName, HeaderValue};
use chrono::Utc;
use conveyor_service::domain::{Provider, Repo, Status, Trigger};
use conveyor_service::providers::{
    CommitState, CommitStatusReport, GenericProvider, GitProvider, Providers, sign_sha256,
};
use serde_json::json;

const SECRET: &[u8] = b"shared with the sender";

fn signed(body: &[u8]) -> HeaderMap {
    let mut map = HeaderMap::new();
    map.insert(
        HeaderName::from_static("x-conveyor-signature-256"),
        HeaderValue::from_str(&sign_sha256(body, SECRET)).expect("header"),
    );
    map
}

fn body(extra: serde_json::Value) -> Vec<u8> {
    let mut payload = json!({
        "delivery_id": "d-1",
        "owner": "me",
        "name": "thing",
        "ref": "refs/heads/master",
        "sha": "a".repeat(40),
    });
    for (key, value) in extra.as_object().expect("an object") {
        payload[key] = value.clone();
    }
    payload.to_string().into_bytes()
}

#[test]
fn a_signed_delivery_is_accepted() {
    let generic = GenericProvider::new();
    let payload = body(json!({}));
    assert!(generic.verify(&signed(&payload), &payload, SECRET));
}

#[test]
fn a_delivery_signed_with_another_secret_is_rejected() {
    let generic = GenericProvider::new();
    let payload = body(json!({}));

    let mut headers = HeaderMap::new();
    headers.insert(
        HeaderName::from_static("x-conveyor-signature-256"),
        HeaderValue::from_str(&sign_sha256(&payload, b"wrong")).expect("header"),
    );

    assert!(!generic.verify(&headers, &payload, SECRET));
}

#[test]
fn generic_does_not_accept_githubs_header() {
    // Each provider reads its own, so a delivery aimed at one cannot be
    // replayed against the other.
    let generic = GenericProvider::new();
    let payload = body(json!({}));

    let mut headers = HeaderMap::new();
    headers.insert(
        HeaderName::from_static("x-hub-signature-256"),
        HeaderValue::from_str(&sign_sha256(&payload, SECRET)).expect("header"),
    );

    assert!(!generic.verify(&headers, &payload, SECRET));
}

#[test]
fn a_minimal_payload_becomes_a_push() {
    let generic = GenericProvider::new();
    let payload = body(json!({}));

    let event = generic
        .parse(&signed(&payload), &payload)
        .expect("parses")
        .expect("should trigger");

    assert_eq!(event.trigger, Trigger::Push);
    assert_eq!(event.owner, "me");
    assert_eq!(event.name, "thing");
    assert_eq!(event.git_ref, "refs/heads/master");
    assert_eq!(event.delivery_id, "d-1");
    assert!(!event.from_fork);
}

#[test]
fn the_event_kind_can_be_named() {
    let generic = GenericProvider::new();
    let payload = body(json!({ "event": "pull_request" }));

    let event = generic.parse(&signed(&payload), &payload).unwrap().unwrap();
    assert_eq!(event.trigger, Trigger::PullRequest);
}

#[test]
fn an_unknown_event_kind_is_an_error() {
    let generic = GenericProvider::new();
    let payload = body(json!({ "event": "wat" }));
    assert!(generic.parse(&signed(&payload), &payload).is_err());
}

#[test]
fn a_payload_missing_a_required_field_is_an_error() {
    let generic = GenericProvider::new();
    let payload = br#"{"owner":"me"}"#;
    assert!(generic.parse(&HeaderMap::new(), payload).is_err());
}

#[tokio::test]
async fn reporting_a_status_is_a_no_op_rather_than_a_failure() {
    // A repository registered as `generic` is one conveyor has no API for.
    // Failing every run's final step over that would be wrong.
    let generic = GenericProvider::new();
    let repo = Repo {
        id: "r".to_string(),
        provider: Provider::Generic,
        owner: "me".to_string(),
        name: "thing".to_string(),
        clone_url: "file:///tmp/x".to_string(),
        default_branch: "master".to_string(),
        registered_by: "someone".to_string(),
        project_id: "project-1".to_string(),
        enabled: true,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    let report = CommitStatusReport::new(Status::Failed, "a stage failed");
    assert!(
        generic
            .report_status(&repo, "abc1234", &report)
            .await
            .is_ok()
    );
}

// ---------------------------------------------------------------------------
// Status mapping
// ---------------------------------------------------------------------------

#[test]
fn a_skipped_run_reads_as_success() {
    // Nothing ran, so nothing is wrong. A pull request whose pipeline excluded
    // every stage should not be blocked by a red mark.
    assert_eq!(
        CommitState::from_status(Status::Skipped),
        CommitState::Success
    );
}

#[test]
fn a_cancelled_run_reads_as_an_error_rather_than_a_failure() {
    // The code was never shown to be broken; somebody stopped the build.
    assert_eq!(
        CommitState::from_status(Status::Cancelled),
        CommitState::Error
    );
}

#[test]
fn the_remaining_statuses_map_as_they_read() {
    assert_eq!(
        CommitState::from_status(Status::Queued),
        CommitState::Pending
    );
    assert_eq!(
        CommitState::from_status(Status::Running),
        CommitState::Pending
    );
    assert_eq!(
        CommitState::from_status(Status::Success),
        CommitState::Success
    );
    assert_eq!(
        CommitState::from_status(Status::Failed),
        CommitState::Failure
    );
}

// ---------------------------------------------------------------------------
// The registry
// ---------------------------------------------------------------------------

#[test]
fn providers_resolve_by_the_name_a_webhook_arrives_on() {
    let providers = Providers::from_env();

    assert_eq!(
        providers.by_name("github").map(|(kind, _)| kind),
        Some(Provider::GitHub)
    );
    assert_eq!(
        providers.by_name("generic").map(|(kind, _)| kind),
        Some(Provider::Generic)
    );
    assert!(providers.by_name("gitlab").is_none());
}

#[test]
fn each_provider_reports_its_own_name() {
    let providers = Providers::from_env();
    assert_eq!(providers.get(Provider::GitHub).name(), "github");
    assert_eq!(providers.get(Provider::Generic).name(), "generic");
}
