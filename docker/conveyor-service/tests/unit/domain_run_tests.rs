//! Unit tests for `domain/run.rs`.

use chrono::Utc;
use conveyor_service::domain::{Run, Status, Trigger};

fn run_at(git_ref: &str, sha: &str) -> Run {
    Run {
        id: "run-1".to_string(),
        repo_id: "repo-1".to_string(),
        trigger: Trigger::Push,
        git_ref: git_ref.to_string(),
        sha: sha.to_string(),
        message: None,
        delivery_id: None,
        status: Status::Queued,
        queued_at: Utc::now(),
        started_at: None,
        finished_at: None,
        claimed_by: None,
        claimed_at: None,
        attempt: 0,
        error: None,
        resumed_from: None,
    }
}

#[test]
fn ref_name_strips_the_branch_prefix() {
    assert_eq!(run_at("refs/heads/master", "abc").ref_name(), "master");
    assert_eq!(
        run_at("refs/heads/feature/nested", "abc").ref_name(),
        "feature/nested"
    );
}

#[test]
fn ref_name_strips_the_tag_prefix() {
    assert_eq!(run_at("refs/tags/v1.2.3", "abc").ref_name(), "v1.2.3");
}

#[test]
fn ref_name_passes_through_a_bare_name() {
    // A manual trigger names a branch directly rather than sending a full ref.
    assert_eq!(run_at("master", "abc").ref_name(), "master");
}

#[test]
fn tags_are_distinguished_from_branches() {
    assert!(run_at("refs/tags/v1.0.0", "abc").is_tag());
    assert!(!run_at("refs/heads/v1.0.0", "abc").is_tag());
    assert!(!run_at("master", "abc").is_tag());
}

#[test]
fn short_sha_takes_the_first_seven_characters() {
    assert_eq!(
        run_at("refs/heads/master", "0123456789abcdef").short_sha(),
        "0123456"
    );
}

#[test]
fn short_sha_of_a_shorter_sha_does_not_panic() {
    // Nothing guarantees a provider sends a full 40-character sha, and slicing
    // past the end would take down the page that renders the run.
    assert_eq!(run_at("refs/heads/master", "abc").short_sha(), "abc");
    assert_eq!(run_at("refs/heads/master", "").short_sha(), "");
}

#[test]
fn triggers_parse_back_from_what_they_render() {
    for trigger in [Trigger::Push, Trigger::PullRequest, Trigger::Manual] {
        assert_eq!(Trigger::parse(trigger.as_str()), Some(trigger));
    }
}

#[test]
fn pull_request_accepts_the_spellings_providers_actually_send() {
    assert_eq!(Trigger::parse("pull_request"), Some(Trigger::PullRequest));
    assert_eq!(Trigger::parse("pullRequest"), Some(Trigger::PullRequest));
    assert_eq!(Trigger::parse("PR"), Some(Trigger::PullRequest));
    assert_eq!(Trigger::parse("merge"), None);
}
