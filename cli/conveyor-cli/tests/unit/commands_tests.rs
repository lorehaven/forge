use conveyor_cli::cli::*;
use conveyor_cli::client::Client;
use conveyor_cli::commands::*;
use quench_cli::prelude::Tone;
use serde_json::json;

#[test]
fn split_slug_splits_owner_and_name() {
    let (owner, name) = split_slug("lorehaven/forge").unwrap();
    assert_eq!(owner, "lorehaven");
    assert_eq!(name, "forge");
}

#[test]
fn split_slug_rejects_a_bare_name() {
    let err = split_slug("forge").unwrap_err();
    assert!(err.to_string().contains("owner/name"));
}

#[test]
fn split_slug_takes_only_the_first_slash() {
    let (owner, name) = split_slug("owner/name/extra").unwrap();
    assert_eq!(owner, "owner");
    assert_eq!(name, "name/extra");
}

#[test]
fn string_reads_a_present_key() {
    let value = json!({ "id": "abc123" });
    assert_eq!(string(&value, "id"), "abc123");
}

#[test]
fn string_defaults_to_empty_for_a_missing_key() {
    let value = json!({ "id": "abc123" });
    assert_eq!(string(&value, "missing"), "");
}

#[test]
fn string_defaults_to_empty_for_a_non_string_value() {
    let value = json!({ "count": 5 });
    assert_eq!(string(&value, "count"), "");
}

#[test]
fn short_takes_the_first_seven_characters_of_the_sha() {
    let run = json!({ "sha": "abcdef1234567890" });
    assert_eq!(short(&run), "abcdef1");
}

#[test]
fn short_of_a_shorter_sha_is_unchanged() {
    let run = json!({ "sha": "abc" });
    assert_eq!(short(&run), "abc");
}

#[test]
fn short_of_a_missing_sha_is_empty() {
    let run = json!({});
    assert_eq!(short(&run), "");
}

#[test]
fn tone_for_maps_terminal_statuses() {
    assert!(matches!(tone_for("success"), Tone::Success));
    assert!(matches!(tone_for("failed"), Tone::Error));
    assert!(matches!(tone_for("cancelled"), Tone::Error));
    assert!(matches!(tone_for("running"), Tone::Info));
    assert!(matches!(tone_for("queued"), Tone::Info));
}

fn write_temp(name: &str, contents: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "conveyor-validate-test-{name}-{}",
        std::process::id()
    ));
    std::fs::write(&path, contents).unwrap();
    path
}

#[test]
fn validate_accepts_a_well_formed_pipeline() {
    let path = write_temp(
        "valid",
        r#"
            on = { push = ["master"] }

            [[stage]]
            name = "check"

            [[stage.job]]
            name = "format"
            image = "rust:latest"
            steps = [{ run = "cargo fmt --check" }]
            "#,
    );

    let result = validate(&ValidateArgs {
        path: path.display().to_string(),
    });
    assert!(result.is_ok(), "{result:?}");

    std::fs::remove_file(&path).ok();
}

#[test]
fn validate_rejects_malformed_toml() {
    let path = write_temp("malformed", "not = [valid");

    let result = validate(&ValidateArgs {
        path: path.display().to_string(),
    });
    assert!(result.is_err());

    std::fs::remove_file(&path).ok();
}

#[test]
fn validate_reports_a_missing_file() {
    let path = std::env::temp_dir().join(format!(
        "conveyor-validate-test-missing-{}-does-not-exist",
        std::process::id()
    ));

    let result = validate(&ValidateArgs {
        path: path.display().to_string(),
    });
    let err = result.unwrap_err();
    assert!(err.to_string().contains("could not read"));
}

// ------------------------------------------------------------------
// The rest of this module talks to a `wiremock` server through a
// `Client::for_tests`-built client - never a real network.
// ------------------------------------------------------------------

use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn client_for(server: &MockServer) -> Client {
    Client::for_tests(server.uri())
}

#[tokio::test]
async fn resolve_repo_returns_a_bare_reference_unchanged_without_a_lookup() {
    // No mock mounted at all: a match here would panic on an
    // unexpected request, proving the id-shaped fast path never calls
    // the network.
    let server = MockServer::start().await;
    let client = client_for(&server).await;
    assert_eq!(resolve_repo(&client, "abc-123").await.unwrap(), "abc-123");
}

#[tokio::test]
async fn resolve_repo_looks_up_an_owner_slash_name_reference() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/repos"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            { "id": "repo-1", "owner": "lorehaven", "name": "forge" }
        ])))
        .mount(&server)
        .await;

    let client = client_for(&server).await;
    assert_eq!(
        resolve_repo(&client, "lorehaven/forge").await.unwrap(),
        "repo-1"
    );
}

#[tokio::test]
async fn resolve_repo_errors_when_the_slug_is_not_registered() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/repos"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&server)
        .await;

    let client = client_for(&server).await;
    let err = resolve_repo(&client, "lorehaven/forge").await.unwrap_err();
    assert!(err.to_string().contains("is not registered"), "{err}");
}

#[tokio::test]
async fn repo_add_posts_the_split_slug_and_resolved_project() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/repos"))
        .and(body_json(json!({
            "provider": "github",
            "owner": "lorehaven",
            "name": "forge",
            "clone_url": "https://example.com/forge.git",
            "default_branch": "master",
            "project_id": "proj-1",
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id": "repo-9"})))
        .mount(&server)
        .await;

    let client = client_for(&server).await;
    repo(
        &client,
        &RepoCommands::Add(RepoAddArgs {
            slug: "lorehaven/forge".to_string(),
            clone_url: "https://example.com/forge.git".to_string(),
            project: "proj-1".to_string(),
            provider: "github".to_string(),
            default_branch: "master".to_string(),
        }),
    )
    .await
    .expect("repo add");
}

#[tokio::test]
async fn repo_add_rejects_a_slug_without_a_slash() {
    let server = MockServer::start().await;
    let client = client_for(&server).await;
    let err = repo(
        &client,
        &RepoCommands::Add(RepoAddArgs {
            slug: "forge".to_string(),
            clone_url: "https://example.com/forge.git".to_string(),
            project: "proj-1".to_string(),
            provider: "github".to_string(),
            default_branch: "master".to_string(),
        }),
    )
    .await
    .unwrap_err();
    assert!(err.to_string().contains("owner/name"), "{err}");
}

#[tokio::test]
async fn repo_list_prints_nothing_special_when_empty() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/repos"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&server)
        .await;

    let client = client_for(&server).await;
    repo(&client, &RepoCommands::List).await.expect("repo list");
}

#[tokio::test]
async fn repo_enable_and_disable_resolve_then_post_the_flag() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/repos"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            { "id": "repo-1", "owner": "lorehaven", "name": "forge" }
        ])))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v1/repos/repo-1/enabled"))
        .and(body_json(json!({"enabled": true})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v1/repos/repo-1/enabled"))
        .and(body_json(json!({"enabled": false})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(&server)
        .await;

    let client = client_for(&server).await;
    repo(
        &client,
        &RepoCommands::Enable(RepoEnableArgs {
            repo: "lorehaven/forge".to_string(),
        }),
    )
    .await
    .expect("enable");
    repo(
        &client,
        &RepoCommands::Disable(RepoEnableArgs {
            repo: "lorehaven/forge".to_string(),
        }),
    )
    .await
    .expect("disable");
}

#[tokio::test]
async fn repo_move_and_set_branch_and_remove_resolve_then_act() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/repos"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            { "id": "repo-1", "owner": "lorehaven", "name": "forge" }
        ])))
        .mount(&server)
        .await;
    Mock::given(method("PATCH"))
        .and(path("/api/v1/repos/repo-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/api/v1/repos/repo-1"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;

    let client = client_for(&server).await;
    repo(
        &client,
        &RepoCommands::Move(RepoMoveArgs {
            repo: "lorehaven/forge".to_string(),
            project: "proj-2".to_string(),
        }),
    )
    .await
    .expect("move");
    repo(
        &client,
        &RepoCommands::SetBranch(RepoSetBranchArgs {
            repo: "lorehaven/forge".to_string(),
            branch: "main".to_string(),
        }),
    )
    .await
    .expect("set-branch");
    repo(
        &client,
        &RepoCommands::Remove(RepoRefArgs {
            repo: "lorehaven/forge".to_string(),
        }),
    )
    .await
    .expect("remove");
}

#[tokio::test]
async fn project_add_list_show_rename_move_remove() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/projects"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id": "proj-1"})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/projects"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/projects/proj-1"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"id": "proj-1", "path": "root/child"})),
        )
        .mount(&server)
        .await;
    Mock::given(method("PATCH"))
        .and(path("/api/v1/projects/proj-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/api/v1/projects/proj-1"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;

    let client = client_for(&server).await;
    project(
        &client,
        &ProjectCommands::Add(ProjectAddArgs {
            name: "child".to_string(),
            parent: None,
        }),
    )
    .await
    .expect("add");
    project(
        &client,
        &ProjectCommands::List(ProjectListArgs { parent: None }),
    )
    .await
    .expect("list");
    project(
        &client,
        &ProjectCommands::List(ProjectListArgs {
            parent: Some("proj-1".to_string()),
        }),
    )
    .await
    .expect("list with parent");
    project(
        &client,
        &ProjectCommands::Show(ProjectRefArgs {
            project: "proj-1".to_string(),
        }),
    )
    .await
    .expect("show");
    project(
        &client,
        &ProjectCommands::Rename(ProjectRenameArgs {
            project: "proj-1".to_string(),
            name: "renamed".to_string(),
        }),
    )
    .await
    .expect("rename");
    project(
        &client,
        &ProjectCommands::Move(ProjectMoveArgs {
            project: "proj-1".to_string(),
            parent: Some("proj-2".to_string()),
            to_root: false,
        }),
    )
    .await
    .expect("move to parent");
    project(
        &client,
        &ProjectCommands::Move(ProjectMoveArgs {
            project: "proj-1".to_string(),
            parent: None,
            to_root: true,
        }),
    )
    .await
    .expect("move to root");
    project(
        &client,
        &ProjectCommands::Remove(ProjectRefArgs {
            project: "proj-1".to_string(),
        }),
    )
    .await
    .expect("remove");
}

#[tokio::test]
async fn project_move_requires_a_parent_or_to_root() {
    let server = MockServer::start().await;
    let client = client_for(&server).await;
    let err = project(
        &client,
        &ProjectCommands::Move(ProjectMoveArgs {
            project: "proj-1".to_string(),
            parent: None,
            to_root: false,
        }),
    )
    .await
    .unwrap_err();
    assert!(err.to_string().contains("--parent or --to-root"), "{err}");
}

#[tokio::test]
async fn run_queues_a_run_and_prints_its_id() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/repos/repo-1/runs"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"id": "run-1", "git_ref": "master", "sha": "abc"})),
        )
        .mount(&server)
        .await;

    let client = client_for(&server).await;
    run(
        &client,
        &RunArgs {
            repo: "repo-1".to_string(),
            git_ref: Some("master".to_string()),
            sha: None,
            wait: false,
        },
    )
    .await
    .expect("run");
}

#[tokio::test]
async fn run_with_wait_polls_until_the_run_succeeds() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/repos/repo-1/runs"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"id": "run-1", "git_ref": "m"})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/runs/run-1"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"id": "run-1", "status": "success"})),
        )
        .mount(&server)
        .await;

    let client = client_for(&server).await;
    run(
        &client,
        &RunArgs {
            repo: "repo-1".to_string(),
            git_ref: None,
            sha: None,
            wait: true,
        },
    )
    .await
    .expect("run --wait");
}

#[tokio::test]
async fn run_with_wait_fails_when_the_run_fails() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/repos/repo-1/runs"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"id": "run-1", "git_ref": "m"})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/runs/run-1"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(
                json!({"id": "run-1", "status": "failed", "error": "step 2 exited 1"}),
            ),
        )
        .mount(&server)
        .await;

    let client = client_for(&server).await;
    let err = run(
        &client,
        &RunArgs {
            repo: "repo-1".to_string(),
            git_ref: None,
            sha: None,
            wait: true,
        },
    )
    .await
    .unwrap_err();
    assert!(err.to_string().contains("step 2 exited 1"), "{err}");
}

#[tokio::test]
async fn runs_lists_recent_runs_and_can_scope_to_a_repo() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/repos"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            { "id": "repo-1", "owner": "lorehaven", "name": "forge" }
        ])))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/runs"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&server)
        .await;

    let client = client_for(&server).await;
    runs(
        &client,
        &RunsArgs {
            repo: None,
            limit: 20,
        },
    )
    .await
    .expect("runs");
    runs(
        &client,
        &RunsArgs {
            repo: Some("lorehaven/forge".to_string()),
            limit: 5,
        },
    )
    .await
    .expect("runs scoped to a repo");
}

#[tokio::test]
async fn show_prints_the_run_its_jobs_and_its_artifacts() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
            .and(path("/api/v1/runs/run-1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "run-1",
                "status": "failed",
                "git_ref": "master",
                "sha": "abcdef1234",
                "error": "boom",
                "jobs": [{"id": "job-1", "stage": "build", "name": "compile", "status": "failed", "error": "boom"}],
                "artifacts": [{"name": "binary", "uri": "s3://bucket/binary"}],
            })))
            .mount(&server)
            .await;

    let client = client_for(&server).await;
    show(
        &client,
        &ShowArgs {
            run_id: "run-1".to_string(),
        },
    )
    .await
    .expect("show");
}

#[tokio::test]
async fn cancel_posts_the_cancel_endpoint() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/runs/run-1/cancel"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    let client = client_for(&server).await;
    cancel(
        &client,
        &CancelArgs {
            run_id: "run-1".to_string(),
        },
    )
    .await
    .expect("cancel");
}

#[tokio::test]
async fn logs_prints_stored_lines_for_every_job_in_a_run() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/runs/run-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "run-1",
            "jobs": [{"id": "job-1", "stage": "build", "name": "compile"}],
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/jobs/job-1/logs"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"stream": "stdout", "line": "hello"},
            {"stream": "stderr", "line": "warn"},
        ])))
        .mount(&server)
        .await;

    let client = client_for(&server).await;
    logs(
        &client,
        &LogsArgs {
            id: "run-1".to_string(),
            follow: false,
        },
    )
    .await
    .expect("logs");
}

#[tokio::test]
async fn logs_treats_an_id_that_is_not_a_run_as_a_job_id() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/runs/job-1"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/jobs/job-1/logs"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&server)
        .await;

    let client = client_for(&server).await;
    logs(
        &client,
        &LogsArgs {
            id: "job-1".to_string(),
            follow: false,
        },
    )
    .await
    .expect("logs on a bare job id");
}

#[tokio::test]
async fn logs_follow_reads_the_sse_stream_until_the_done_event() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/runs/run-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "run-1",
            "jobs": [{"id": "job-1", "stage": "build", "name": "compile"}],
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/jobs/job-1/stream"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("event: log\ndata: building\n\nevent: done\ndata: \n\n"),
        )
        .mount(&server)
        .await;

    let client = client_for(&server).await;
    logs(
        &client,
        &LogsArgs {
            id: "run-1".to_string(),
            follow: true,
        },
    )
    .await
    .expect("logs --follow");
}

#[tokio::test]
async fn secret_set_list_and_remove_at_estate_scope() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path("/api/v1/secrets/db-password"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/secrets"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/api/v1/secrets/db-password"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;

    let client = client_for(&server).await;
    secret(
        &client,
        &SecretCommands::Set(SecretSetArgs {
            name: "db-password".to_string(),
            value: Some("hunter2".to_string()),
            repo: None,
        }),
    )
    .await
    .expect("set");
    secret(
        &client,
        &SecretCommands::List(SecretScopeArgs { repo: None }),
    )
    .await
    .expect("list");
    secret(
        &client,
        &SecretCommands::Remove(SecretRemoveArgs {
            name: "db-password".to_string(),
            repo: None,
        }),
    )
    .await
    .expect("remove");
}

#[tokio::test]
async fn secret_set_scoped_to_a_repo_resolves_it_first() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/repos"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            { "id": "repo-1", "owner": "lorehaven", "name": "forge" }
        ])))
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/api/v1/repos/repo-1/secrets/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(&server)
        .await;

    let client = client_for(&server).await;
    secret(
        &client,
        &SecretCommands::Set(SecretSetArgs {
            name: "token".to_string(),
            value: Some("v".to_string()),
            repo: Some("lorehaven/forge".to_string()),
        }),
    )
    .await
    .expect("set scoped to a repo");
}

#[tokio::test]
async fn credential_set_show_and_remove_scoped_to_a_project() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path("/api/v1/projects/proj-1/credentials"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"preview": "ghp_****"})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
            .and(path("/api/v1/projects/proj-1/credentials"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "name": "ci-token", "preview": "ghp_****", "username": "x-access-token", "created_by": "alice",
            })))
            .mount(&server)
            .await;
    Mock::given(method("DELETE"))
        .and(path("/api/v1/projects/proj-1/credentials"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;

    let client = client_for(&server).await;
    credential(
        &client,
        &CredentialCommands::Set(CredentialSetArgs {
            name: "ci-token".to_string(),
            token: Some("secret-token".to_string()),
            git_username: "x-access-token".to_string(),
            repo: None,
            project: Some("proj-1".to_string()),
        }),
    )
    .await
    .expect("set");
    credential(
        &client,
        &CredentialCommands::Show(CredentialScopeArgs {
            repo: None,
            project: Some("proj-1".to_string()),
        }),
    )
    .await
    .expect("show");
    credential(
        &client,
        &CredentialCommands::Remove(CredentialScopeArgs {
            repo: None,
            project: Some("proj-1".to_string()),
        }),
    )
    .await
    .expect("remove");
}

#[tokio::test]
async fn credential_show_reports_none_set_for_a_null_response() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/projects/proj-1/credentials"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!(null)))
        .mount(&server)
        .await;

    let client = client_for(&server).await;
    credential(
        &client,
        &CredentialCommands::Show(CredentialScopeArgs {
            repo: None,
            project: Some("proj-1".to_string()),
        }),
    )
    .await
    .expect("show with nothing set");
}

#[tokio::test]
async fn credential_rejects_both_repo_and_project_scope() {
    let server = MockServer::start().await;
    let client = client_for(&server).await;
    let err = credential(
        &client,
        &CredentialCommands::Show(CredentialScopeArgs {
            repo: Some("lorehaven/forge".to_string()),
            project: Some("proj-1".to_string()),
        }),
    )
    .await
    .unwrap_err();
    assert!(err.to_string().contains("not both"), "{err}");
}

#[tokio::test]
async fn credential_requires_a_scope() {
    let server = MockServer::start().await;
    let client = client_for(&server).await;
    let err = credential(
        &client,
        &CredentialCommands::Show(CredentialScopeArgs {
            repo: None,
            project: None,
        }),
    )
    .await
    .unwrap_err();
    assert!(err.to_string().contains("needs a scope"), "{err}");
}
