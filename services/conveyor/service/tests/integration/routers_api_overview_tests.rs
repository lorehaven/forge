//! HTTP-layer coverage for `routers::api::overview` and the reads the browser client added to `projects` and `runs`: what the
//! htmx pages used to compute from the database inside the page.

use crate::support::{database, json_body, json_req, req, skipped};
use conveyor_service::config::ConveyorConfig;
use conveyor_service::domain::{Artifact, Trigger};
use conveyor_service::providers::Providers;
use conveyor_service::routers::api;
use conveyor_service::scheduler::projects::{self, NewProject};
use conveyor_service::scheduler::queue::{self, NewRun};
use conveyor_service::scheduler::repos::{self, NewRepo};
use quench_auth::domain::jwt::JwtConfig;
use quench_core::time::Time;
use quench_db::prelude::{Database, Db};
use quench_http::di::ContainerBuilder;
use quench_http::endpoint::Endpoint;
use quench_http::http::{Method, StatusCode};
use std::sync::Arc;

async fn app(
    db: Db,
    config: ConveyorConfig,
) -> (Arc<dyn Endpoint>, Arc<quench_http::di::Container>) {
    db.execute("INSERT INTO auth.users (username, password, roles) VALUES ('admin', 'x', '[]'::jsonb) ON CONFLICT DO NOTHING")
        .await
        .expect("seed the admin user");
    api::register_routes();
    let container = ContainerBuilder::new()
        .provide(db)
        .provide(JwtConfig::for_tests())
        .provide(config)
        .provide_arc(Arc::new(Providers::from_env()))
        .build()
        .await
        .unwrap();
    (
        quench_starter::http::discover_and_mount("/"),
        Arc::new(container),
    )
}

async fn project(db: &Db, name: &str, parent: Option<&str>) -> String {
    projects::create(
        db,
        &NewProject {
            name: name.to_string(),
            parent_id: parent.map(str::to_string),
        },
    )
    .await
    .expect("create the project")
    .id
}

async fn repo(db: &Db, name: &str, project_id: &str) -> conveyor_service::domain::Repo {
    repos::create(
        db,
        &NewRepo {
            provider: conveyor_service::domain::Provider::Generic,
            owner: "tests".to_string(),
            name: name.to_string(),
            clone_url: format!("https://example.test/{name}.git"),
            default_branch: "master".to_string(),
            registered_by: crate::support::TEST_USER.to_string(),
            project_id: project_id.to_string(),
        },
    )
    .await
    .expect("create the repo")
}

async fn run(db: &Db, repo_id: &str, sha_digit: char) -> conveyor_service::domain::Run {
    queue::enqueue(
        db,
        &NewRun {
            repo_id: repo_id.to_string(),
            trigger: Trigger::Push,
            git_ref: "refs/heads/master".to_string(),
            sha: sha_digit.to_string().repeat(40),
            message: Some("a commit".to_string()),
            delivery_id: None,
            resumed_from: None,
            stages: None,
        },
    )
    .await
    .expect("enqueue")
    .run()
    .clone()
}

async fn get(
    app: &(Arc<dyn Endpoint>, Arc<quench_http::di::Container>),
    path: &str,
) -> (StatusCode, quench_serde::Value) {
    let resp = app.0.call(req(Method::GET, path, &app.1)).await;
    let status = resp.status();
    (status, json_body(resp).await)
}

#[quench_test::test]
async fn access_lists_every_project_when_auth_is_off() {
    let Some((db, _guard)) = database().await else {
        return skipped("access_lists_every_project_when_auth_is_off");
    };
    let root = project(&db, "root", None).await;
    let leaf = project(&db, "leaf", Some(&root)).await;
    let app = app(db, ConveyorConfig::default()).await;

    let (status, body) = get(&app, "/api/v1/access").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["username"], "admin");
    for key in ["read", "write"] {
        let ids: Vec<&str> = body[key]
            .as_array()
            .expect("an array")
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        assert!(
            ids.contains(&root.as_str()) && ids.contains(&leaf.as_str()),
            "{key}: {ids:?}"
        );
    }
}

#[quench_test::test]
async fn projects_can_be_listed_whole_not_one_level_at_a_time() {
    let Some((db, _guard)) = database().await else {
        return skipped("projects_can_be_listed_whole_not_one_level_at_a_time");
    };
    let root = project(&db, "root", None).await;
    project(&db, "deep", Some(&root)).await;
    let app = app(db, ConveyorConfig::default()).await;

    let (_, top) = get(&app, "/api/v1/projects").await;
    assert_eq!(top.as_array().unwrap().len(), 1);
    let (status, all) = get(&app, "/api/v1/projects?all=true").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(all.as_array().unwrap().len(), 2);
}

#[quench_test::test]
async fn the_front_pages_runs_are_capped_per_repository_and_scoped_to_a_project() {
    let Some((db, _guard)) = database().await else {
        return skipped("the_front_pages_runs_are_capped_per_repository_and_scoped_to_a_project");
    };
    let one = project(&db, "one", None).await;
    let two = project(&db, "two", None).await;
    let a = repo(&db, "a", &one).await;
    let b = repo(&db, "b", &two).await;
    run(&db, &a.id, 'a').await;
    let newest_a = run(&db, &a.id, 'b').await;
    run(&db, &b.id, 'c').await;
    let app = app(db, ConveyorConfig::default()).await;

    // One run per repository by default, whatever its age.
    let (status, all) = get(&app, "/api/v1/overview/runs").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(all.as_array().unwrap().len(), 2, "{all:?}");

    let (_, scoped) = get(&app, &format!("/api/v1/overview/runs?project={one}")).await;
    let runs = scoped.as_array().unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0]["id"], newest_a.id.as_str());

    let (status, _) = get(&app, "/api/v1/overview/runs?project=nope").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[quench_test::test]
async fn the_history_is_paged_and_scoped() {
    let Some((db, _guard)) = database().await else {
        return skipped("the_history_is_paged_and_scoped");
    };
    let one = project(&db, "one", None).await;
    let two = project(&db, "two", None).await;
    let a = repo(&db, "a", &one).await;
    let b = repo(&db, "b", &two).await;
    for digit in ['1', '2', '3'] {
        run(&db, &a.id, digit).await;
    }
    run(&db, &b.id, '4').await;
    let config = ConveyorConfig {
        runs_page_size: 3,
        ..ConveyorConfig::default()
    };
    let app = app(db, config).await;

    let (status, first) = get(&app, "/api/v1/overview/history").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(first["total"], 4);
    assert_eq!(first["pages"], 2);
    assert_eq!(first["page"], 1);
    assert_eq!(first["runs"].as_array().unwrap().len(), 3);

    let (_, second) = get(&app, "/api/v1/overview/history?page=2").await;
    assert_eq!(second["runs"].as_array().unwrap().len(), 1);

    let (_, scoped) = get(&app, &format!("/api/v1/overview/history?project={two}")).await;
    assert_eq!(scoped["total"], 1);
    assert_eq!(scoped["pages"], 1);

    // A page past the end is an empty page, not an error; an empty history is still page 1 of 1.
    let (_, past) = get(&app, "/api/v1/overview/history?page=9").await;
    assert!(past["runs"].as_array().unwrap().is_empty());
    let (status, _) = get(&app, "/api/v1/overview/history?project=nope").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[quench_test::test]
async fn a_repository_that_never_ran_has_a_scan_with_nothing_in_it() {
    let Some((db, _guard)) = database().await else {
        return skipped("a_repository_that_never_ran_has_a_scan_with_nothing_in_it");
    };
    let p = project(&db, "p", None).await;
    let a = repo(&db, "a", &p).await;
    let app = app(db, ConveyorConfig::default()).await;

    let (status, chips) = get(&app, "/api/v1/scan").await;
    assert_eq!(status, StatusCode::OK);
    let rows = chips.as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["repo_id"], a.id.as_str());
    assert!(rows[0]["checks"].as_array().unwrap().is_empty());

    let (status, scan) = get(&app, &format!("/api/v1/repos/{}/scan", a.id)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(scan["run"].is_null());
    assert!(scan["checks"].as_array().unwrap().is_empty());

    let (status, _) = get(&app, "/api/v1/repos/nope/scan").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[quench_test::test]
async fn the_scan_names_the_newest_run_and_the_scanner_findings_may_be_absent() {
    let Some((db, _guard)) = database().await else {
        return skipped("the_scan_names_the_newest_run_and_the_scanner_findings_may_be_absent");
    };
    let p = project(&db, "p", None).await;
    let a = repo(&db, "a", &p).await;
    let newest = run(&db, &a.id, 'f').await;
    let app = app(db, ConveyorConfig::default()).await;

    let (_, scan) = get(&app, &format!("/api/v1/repos/{}/scan", a.id)).await;
    assert_eq!(scan["run"]["id"], newest.id.as_str());

    let (status, _) = get(&app, &format!("/api/v1/repos/{}/scan/findings", a.id)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    // Only the tests and the security gate keep a report page.
    let (status, _) = get(&app, &format!("/api/v1/repos/{}/scan/lint/report", a.id)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = get(&app, &format!("/api/v1/repos/{}/scan/tests/report", a.id)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[quench_test::test]
async fn credentials_are_listed_without_their_tokens() {
    let Some((db, _guard)) = database().await else {
        return skipped("credentials_are_listed_without_their_tokens");
    };
    quench_core::env::set(
        "CONVEYOR_CREDENTIAL_KEY",
        "202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f",
    );
    let p = project(&db, "p", None).await;
    let a = repo(&db, "a", &p).await;
    let app = app(db, ConveyorConfig::default()).await;

    let (status, none) = get(&app, "/api/v1/credentials").await;
    assert_eq!(status, StatusCode::OK);
    assert!(none.as_array().unwrap().is_empty());

    for path in [
        format!("/api/v1/projects/{p}/credentials"),
        format!("/api/v1/repos/{}/credentials", a.id),
    ] {
        let resp = app
            .0
            .call(json_req(
                Method::PUT,
                &path,
                &quench_serde::json!({ "name": "deploy", "username": "git", "token": "ghp_abcdefgh" }),
                &app.1,
            ))
            .await;
        assert_eq!(resp.status(), StatusCode::OK, "{path}");
    }

    let resp = app
        .0
        .call(req(Method::GET, "/api/v1/credentials", &app.1))
        .await;
    let text = crate::support::body_text(resp).await;
    assert!(!text.contains("ghp_abcdefgh"), "{text}");
    let list: quench_serde::Value = quench_serde::json::from_str(&text).unwrap();
    let rows = list.as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().any(|r| r["project_id"] == p.as_str()));
    assert!(rows.iter().any(|r| r["repo_id"] == a.id.as_str()));
}

#[quench_test::test]
async fn a_runs_artifacts_carry_the_link_a_browser_can_follow() {
    let Some((db, _guard)) = database().await else {
        return skipped("a_runs_artifacts_carry_the_link_a_browser_can_follow");
    };
    let p = project(&db, "p", None).await;
    let a = repo(&db, "a", &p).await;
    let r = run(&db, &a.id, 'e').await;
    let jobs = queue::create_jobs(
        &db,
        &r.id,
        &[conveyor_service::scheduler::queue::PlannedJob {
            stage: "build".to_string(),
            name: "cargo".to_string(),
            needs: Vec::new(),
            status: conveyor_service::domain::Status::Queued,
            error: None,
            reused_from_run: None,
        }],
    )
    .await
    .expect("plan the jobs");
    let job = &jobs[0];
    queue::record_artifact(
        &db,
        &Artifact {
            id: quench_core::uuid::Uuid::new_v4().to_string(),
            run_id: r.id.clone(),
            job_id: job.id.clone(),
            kind: "file".to_string(),
            name: "report.json".to_string(),
            version: None,
            uri: "https://warehouse.test/files/report.json".to_string(),
            digest: Some("sha256:abc".to_string()),
            created_at: Time::now(),
        },
    )
    .await
    .expect("record the artifact");
    let app = app(db, ConveyorConfig::default()).await;

    let (status, detail) = get(&app, &format!("/api/v1/runs/{}", r.id)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(detail["id"], r.id.as_str());
    let artifacts = detail["artifacts"].as_array().unwrap();
    assert_eq!(artifacts.len(), 1);
    assert!(
        artifacts[0]["href"]
            .as_str()
            .is_some_and(|h| h.ends_with("report.json")),
        "{artifacts:?}"
    );
    assert_eq!(artifacts[0]["name"], "report.json");
}
