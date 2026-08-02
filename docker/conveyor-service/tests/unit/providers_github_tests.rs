//! Unit tests for `providers/github.rs` and the signature scheme in
//! `providers/mod.rs`.

use actix_web::http::header::{HeaderMap, HeaderName, HeaderValue};
use chrono::Utc;
use conveyor_service::domain::{Provider, Repo, Status, Trigger};
use conveyor_service::providers::{
    CommitStatusReport, GitHubProvider, GitProvider, ProviderError, sign_sha256,
};
use serde_json::json;

const SECRET: &[u8] = b"it's a secret to everybody";

fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
    let mut map = HeaderMap::new();
    for (name, value) in pairs {
        map.insert(
            HeaderName::from_bytes(name.as_bytes()).expect("header name"),
            HeaderValue::from_str(value).expect("header value"),
        );
    }
    map
}

fn signed(body: &[u8], event: &str) -> HeaderMap {
    headers(&[
        ("x-hub-signature-256", &sign_sha256(body, SECRET)),
        ("x-github-event", event),
        ("x-github-delivery", "delivery-1"),
    ])
}

// ---------------------------------------------------------------------------
// Signatures
// ---------------------------------------------------------------------------

#[test]
fn the_documented_github_test_vector_verifies() {
    // From GitHub's own webhook documentation. If this ever fails, conveyor and
    // GitHub disagree about what the signature covers - which no amount of
    // round-tripping conveyor's own signer would reveal.
    let signature = sign_sha256(b"Hello, World!", b"It's a Secret to Everybody");
    assert_eq!(
        signature,
        "sha256=757107ea0eb2509fc211221cce984b8a37570b6d7586c22c46f4379c8b043e17"
    );
}

#[test]
fn a_correct_signature_is_accepted() {
    let github = GitHubProvider::from_env();
    let body = br#"{"hello":"world"}"#;
    assert!(github.verify(&signed(body, "push"), body, SECRET));
}

#[test]
fn a_signature_for_different_bytes_is_rejected() {
    // The reason the handler takes raw bytes: a body that has been through a
    // deserialiser and back no longer matches what was signed.
    let github = GitHubProvider::from_env();
    let signature = sign_sha256(br#"{"hello":"world"}"#, SECRET);
    let headers = headers(&[
        ("x-hub-signature-256", &signature),
        ("x-github-event", "push"),
        ("x-github-delivery", "d"),
    ]);

    assert!(!github.verify(&headers, br#"{"hello": "world"}"#, SECRET));
}

#[test]
fn a_signature_made_with_another_secret_is_rejected() {
    let github = GitHubProvider::from_env();
    let body = br#"{"hello":"world"}"#;
    let headers = headers(&[
        ("x-hub-signature-256", &sign_sha256(body, b"wrong")),
        ("x-github-event", "push"),
        ("x-github-delivery", "d"),
    ]);

    assert!(!github.verify(&headers, body, SECRET));
}

#[test]
fn a_missing_or_malformed_signature_is_rejected() {
    let github = GitHubProvider::from_env();
    let body = b"{}";

    assert!(!github.verify(&HeaderMap::new(), body, SECRET));
    for bad in ["", "sha256=", "sha256=nothex", "deadbeef", "sha1=deadbeef"] {
        let headers = headers(&[("x-hub-signature-256", bad)]);
        assert!(
            !github.verify(&headers, body, SECRET),
            "{bad:?} was accepted"
        );
    }
}

// ---------------------------------------------------------------------------
// Push
// ---------------------------------------------------------------------------

fn push_body(git_ref: &str, sha: &str, deleted: bool) -> Vec<u8> {
    json!({
        "ref": git_ref,
        "after": sha,
        "deleted": deleted,
        "head_commit": { "message": "fix the thing\n\nwith a longer body" },
        "repository": {
            "name": "forge",
            "full_name": "lorehaven/forge",
            "owner": { "login": "lorehaven" }
        }
    })
    .to_string()
    .into_bytes()
}

#[test]
fn a_push_becomes_a_trigger() {
    let github = GitHubProvider::from_env();
    let body = push_body("refs/heads/master", "a".repeat(40).as_str(), false);

    let event = github
        .parse(&signed(&body, "push"), &body)
        .expect("parses")
        .expect("a push should trigger");

    assert_eq!(event.trigger, Trigger::Push);
    assert_eq!(event.owner, "lorehaven");
    assert_eq!(event.name, "forge");
    assert_eq!(event.git_ref, "refs/heads/master");
    assert_eq!(event.delivery_id, "delivery-1");
    assert!(
        !event.from_fork,
        "a push is always to the repository itself"
    );
}

#[test]
fn only_the_subject_line_of_the_commit_message_is_kept() {
    let github = GitHubProvider::from_env();
    let body = push_body("refs/heads/master", "a".repeat(40).as_str(), false);

    let event = github
        .parse(&signed(&body, "push"), &body)
        .unwrap()
        .unwrap();
    assert_eq!(event.message.as_deref(), Some("fix the thing"));
}

#[test]
fn a_deleted_branch_triggers_nothing() {
    // Its `after` is all zeros, which would fail the checkout in a way nobody
    // could interpret.
    let github = GitHubProvider::from_env();
    let body = push_body("refs/heads/gone", &"0".repeat(40), true);

    assert!(
        github
            .parse(&signed(&body, "push"), &body)
            .expect("parses")
            .is_none()
    );
}

#[test]
fn a_zero_sha_triggers_nothing_even_without_the_deleted_flag() {
    let github = GitHubProvider::from_env();
    let body = push_body("refs/heads/gone", &"0".repeat(40), false);

    assert!(
        github
            .parse(&signed(&body, "push"), &body)
            .unwrap()
            .is_none()
    );
}

#[test]
fn a_tag_push_keeps_its_full_ref() {
    let github = GitHubProvider::from_env();
    let body = push_body("refs/tags/v1.2.3", "a".repeat(40).as_str(), false);

    let event = github
        .parse(&signed(&body, "push"), &body)
        .unwrap()
        .unwrap();
    assert_eq!(event.git_ref, "refs/tags/v1.2.3");
}

// ---------------------------------------------------------------------------
// Pull requests
// ---------------------------------------------------------------------------

fn pull_request_body(action: &str, head_repo: Option<&str>) -> Vec<u8> {
    json!({
        "action": action,
        "number": 42,
        "pull_request": {
            "title": "Make it better",
            "head": {
                "ref": "topic",
                "sha": "b".repeat(40),
                "repo": head_repo.map(|name| json!({ "full_name": name }))
            }
        },
        "repository": {
            "name": "forge",
            "full_name": "lorehaven/forge",
            "owner": { "login": "lorehaven" }
        }
    })
    .to_string()
    .into_bytes()
}

#[test]
fn a_pull_request_from_the_same_repository_builds_its_branch() {
    // Its branch exists in the base repository, so `branch == '...'` in a
    // `when` still means what it looks like.
    let github = GitHubProvider::from_env();
    let body = pull_request_body("opened", Some("lorehaven/forge"));

    let event = github
        .parse(&signed(&body, "pull_request"), &body)
        .unwrap()
        .unwrap();

    assert_eq!(event.trigger, Trigger::PullRequest);
    assert_eq!(event.git_ref, "refs/heads/topic");
    assert!(!event.from_fork);
}

#[test]
fn a_pull_request_from_a_fork_is_marked_and_uses_the_published_ref() {
    // A fork's branch does not exist in the base repository; GitHub publishes
    // every pull request's head there as `refs/pull/N/head`.
    let github = GitHubProvider::from_env();
    let body = pull_request_body("opened", Some("someone-else/forge"));

    let event = github
        .parse(&signed(&body, "pull_request"), &body)
        .unwrap()
        .unwrap();

    assert!(event.from_fork);
    assert_eq!(event.git_ref, "refs/pull/42/head");
}

#[test]
fn a_pull_request_with_no_head_repository_is_treated_as_a_fork() {
    // GitHub reports this when the head repository was deleted, which it also
    // does for forks. Treating it as "not a fork" would be the unsafe way round.
    let github = GitHubProvider::from_env();
    let body = pull_request_body("opened", None);

    let event = github
        .parse(&signed(&body, "pull_request"), &body)
        .unwrap()
        .unwrap();
    assert!(event.from_fork);
}

#[test]
fn the_actions_that_change_code_trigger_a_build() {
    let github = GitHubProvider::from_env();
    for action in ["opened", "reopened", "synchronize", "ready_for_review"] {
        let body = pull_request_body(action, Some("lorehaven/forge"));
        assert!(
            github
                .parse(&signed(&body, "pull_request"), &body)
                .unwrap()
                .is_some(),
            "{action} should trigger"
        );
    }
}

#[test]
fn the_actions_that_do_not_change_code_trigger_nothing() {
    let github = GitHubProvider::from_env();
    for action in [
        "labeled",
        "assigned",
        "closed",
        "edited",
        "review_requested",
    ] {
        let body = pull_request_body(action, Some("lorehaven/forge"));
        assert!(
            github
                .parse(&signed(&body, "pull_request"), &body)
                .unwrap()
                .is_none(),
            "{action} should not trigger"
        );
    }
}

// ---------------------------------------------------------------------------
// Other events
// ---------------------------------------------------------------------------

#[test]
fn a_ping_is_accepted_and_triggers_nothing() {
    // GitHub sends one when a hook is created. Treating it as an error would
    // show a red mark on the hook the moment it was set up.
    let github = GitHubProvider::from_env();
    let body = br#"{"zen":"Non-blocking is better than blocking."}"#;

    assert!(github.parse(&signed(body, "ping"), body).unwrap().is_none());
}

#[test]
fn an_event_conveyor_does_not_handle_triggers_nothing() {
    let github = GitHubProvider::from_env();
    let body = b"{}";
    assert!(
        github
            .parse(&signed(body, "issue_comment"), body)
            .unwrap()
            .is_none()
    );
}

#[test]
fn a_delivery_without_the_event_header_is_an_error() {
    let github = GitHubProvider::from_env();
    let headers = headers(&[("x-github-delivery", "d")]);
    assert!(github.parse(&headers, b"{}").is_err());
}

#[test]
fn a_delivery_without_an_id_is_an_error() {
    // The id is what makes a redelivery a no-op rather than a second run.
    let github = GitHubProvider::from_env();
    let headers = headers(&[("x-github-event", "push")]);
    assert!(github.parse(&headers, b"{}").is_err());
}

#[test]
fn a_push_body_that_is_not_a_push_is_an_error() {
    let github = GitHubProvider::from_env();
    let body = br#"{"not":"a push"}"#;
    assert!(github.parse(&signed(body, "push"), body).is_err());
}

#[tokio::test]
async fn reporting_a_status_without_a_token_says_so() {
    // Not a failed run: building without reporting is a reasonable way to run
    // conveyor, and the worker treats this case as a no-op.
    unsafe { std::env::remove_var("CONVEYOR_GITHUB_TOKEN") };
    let github = GitHubProvider::from_env();

    let repo = Repo {
        id: "r".to_string(),
        provider: Provider::GitHub,
        owner: "lorehaven".to_string(),
        name: "forge".to_string(),
        clone_url: "https://example.invalid/x.git".to_string(),
        default_branch: "master".to_string(),
        registered_by: "someone".to_string(),
        project_id: "project-1".to_string(),
        enabled: true,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    let report = CommitStatusReport::new(Status::Success, "all stages passed");

    assert!(matches!(
        github.report_status(&repo, &"a".repeat(40), &report).await,
        Err(ProviderError::NotConfigured("github"))
    ));
}
