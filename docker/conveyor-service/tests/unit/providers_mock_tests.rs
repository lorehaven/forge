//! `providers::mock::MockProvider` - the scriptable provider the worker and
//! webhook tests use. Testing the double itself, not because it's risky, but
//! because an untested test double can silently stop doing what its callers
//! assume.

use actix_web::http::header::HeaderMap;
use conveyor_service::domain::Repo;
use conveyor_service::providers::mock::MockProvider;
use conveyor_service::providers::{CommitState, CommitStatusReport, GitProvider};

fn sample_repo() -> Repo {
    Repo {
        id: "repo-1".to_string(),
        provider: conveyor_service::domain::Provider::Generic,
        owner: "tests".to_string(),
        name: "thing".to_string(),
        clone_url: "file:///tmp/thing".to_string(),
        default_branch: "master".to_string(),
        enabled: true,
        registered_by: "tests".to_string(),
        project_id: "project-1".to_string(),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }
}

#[test]
fn name_is_mock() {
    assert_eq!(MockProvider::new().name(), "mock");
}

#[test]
fn verify_reflects_the_scripted_answer() {
    let provider = MockProvider::new();
    assert!(provider.verify(&HeaderMap::new(), b"", b""));

    provider.set_accept_signatures(false);
    assert!(!provider.verify(&HeaderMap::new(), b"", b""));
}

#[test]
fn parse_returns_the_scripted_event() {
    let provider = MockProvider::new();
    assert!(provider.parse(&HeaderMap::new(), b"").unwrap().is_none());

    let event = MockProvider::sample_event();
    provider.set_event(Some(event.clone()));
    assert_eq!(provider.parse(&HeaderMap::new(), b"").unwrap(), Some(event));
}

#[tokio::test]
async fn report_status_records_every_call_in_order() {
    let provider = MockProvider::new();
    let repo = sample_repo();

    provider
        .report_status(
            &repo,
            "sha-1",
            &CommitStatusReport {
                state: CommitState::Pending,
                description: "starting".to_string(),
                target_url: None,
                context: "conveyor".to_string(),
            },
        )
        .await
        .unwrap();
    provider
        .report_status(
            &repo,
            "sha-1",
            &CommitStatusReport {
                state: CommitState::Success,
                description: "done".to_string(),
                target_url: None,
                context: "conveyor".to_string(),
            },
        )
        .await
        .unwrap();

    let reports = provider.reports();
    assert_eq!(reports.len(), 2);
    assert_eq!(reports[0].state, "pending");
    assert_eq!(reports[0].description, "starting");
    assert_eq!(reports[1].state, "success");
    assert_eq!(reports[1].repo, repo.slug());
}

#[test]
fn sample_event_is_a_push_with_a_message() {
    let event = MockProvider::sample_event();
    assert_eq!(event.trigger, conveyor_service::domain::Trigger::Push);
    assert!(event.message.is_some());
    assert!(!event.from_fork);
}
