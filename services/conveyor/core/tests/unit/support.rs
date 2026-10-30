//! Records for the tests.
#![allow(dead_code)]

use conveyor_core::{Job, Project, Provider, Repo, Run, Status, Trigger};
use quench_core::time::Time;

pub fn at(text: &str) -> Time {
    Time::parse_rfc3339(text).unwrap()
}

pub fn project(id: &str, name: &str, parent: Option<&str>) -> Project {
    Project {
        id: id.into(),
        name: name.into(),
        parent_id: parent.map(Into::into),
        created_at: at("2026-10-29T10:00:00Z"),
        updated_at: at("2026-10-29T10:00:00Z"),
    }
}

pub fn repo(id: &str, project_id: &str) -> Repo {
    Repo {
        id: id.into(),
        provider: Provider::GitHub,
        owner: "lorehaven".into(),
        name: id.into(),
        clone_url: format!("https://example.test/{id}.git"),
        default_branch: "master".into(),
        registered_by: "admin".into(),
        project_id: project_id.into(),
        enabled: true,
        created_at: at("2026-10-29T10:00:00Z"),
        updated_at: at("2026-10-29T10:00:00Z"),
    }
}

pub fn run(id: &str, repo_id: &str, status: Status) -> Run {
    Run {
        id: id.into(),
        repo_id: repo_id.into(),
        trigger: Trigger::Push,
        git_ref: "refs/heads/master".into(),
        sha: "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef".into(),
        message: None,
        delivery_id: None,
        status,
        queued_at: at("2026-10-29T10:00:00Z"),
        started_at: None,
        finished_at: None,
        claimed_by: None,
        claimed_at: None,
        attempt: 0,
        error: None,
        resumed_from: None,
        stages: None,
    }
}

pub fn job(stage: &str, name: &str, needs: &[&str]) -> Job {
    Job {
        id: format!("{stage}-{name}"),
        run_id: "run".into(),
        stage: stage.into(),
        name: name.into(),
        needs: needs.iter().map(ToString::to_string).collect(),
        status: Status::Success,
        exit_code: None,
        started_at: None,
        finished_at: None,
        error: None,
        reused_from_run: None,
    }
}
