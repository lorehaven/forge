//! Triggering runs, and looking at what they did.

use crate::domain::{Run, Trigger};
use crate::routers::api::{ApiError, json_error};
use crate::scheduler::queue::{self, NewRun, QueueError};
use crate::scheduler::repos;
use actix_web::http::StatusCode;
use actix_web::{HttpResponse, Responder, get, post, web};
use quench_db::prelude::Db;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct TriggerRun {
    /// Branch or tag to build. Defaults to the repository's default branch.
    #[serde(default)]
    pub git_ref: Option<String>,
    /// The commit to build. Optional: without it conveyor asks the repository
    /// what the ref currently points at.
    #[serde(default)]
    pub sha: Option<String>,
}

/// Why [`trigger_manual`] could not start a run - shared between the JSON API
/// handler below and the UI's "run now" button, which need to report the same
/// failures in different shapes (a JSON error body vs. a re-rendered page).
#[derive(Debug, thiserror::Error)]
pub enum TriggerError {
    #[error("no such repository")]
    NotFound,
    #[error("this repository is disabled")]
    Disabled,
    #[error("{0}")]
    BadRef(String),
    #[error("{0}")]
    ResolveFailed(String),
    #[error("{0}")]
    BadSha(String),
    #[error(transparent)]
    Queue(#[from] QueueError),
}

/// `ApiError`'s fields are private to `routers::api`, not `pub`, but this
/// module is a descendant of it and so can still build one directly - the
/// same way `queue::QueueError` already does two steps up.
impl From<TriggerError> for ApiError {
    fn from(error: TriggerError) -> Self {
        let TriggerError::Queue(queue_error) = error else {
            let status = match &error {
                TriggerError::NotFound => StatusCode::NOT_FOUND,
                TriggerError::Disabled => StatusCode::CONFLICT,
                TriggerError::BadRef(_) | TriggerError::BadSha(_) => StatusCode::BAD_REQUEST,
                TriggerError::ResolveFailed(_) => StatusCode::BAD_GATEWAY,
                TriggerError::Queue(_) => unreachable!(),
            };
            return Self {
                status,
                message: error.to_string(),
            };
        };
        Self::from(queue_error)
    }
}

/// Starts a run by hand.
///
/// The pipeline's `on` patterns do not gate this - somebody asked for this run
/// by name, and refusing would leave them no way to build a branch the patterns
/// do not cover.
///
/// Without a `sha`, the ref's current commit is resolved once here, so the run
/// records what it actually built rather than whatever the ref moves to before
/// a worker picks it up.
pub(crate) async fn trigger_manual(
    db: &Db,
    repo_id: &str,
    git_ref: Option<String>,
    sha: Option<String>,
) -> Result<Run, TriggerError> {
    let repo = match repos::read(db, repo_id).await {
        Ok(Some(repo)) => repo,
        Ok(None) => return Err(TriggerError::NotFound),
        Err(error) => return Err(TriggerError::Queue(error)),
    };

    if !repo.enabled {
        return Err(TriggerError::Disabled);
    }

    let git_ref = git_ref.unwrap_or_else(|| format!("refs/heads/{}", repo.default_branch));

    crate::workspace::checkout::validate_ref(&git_ref)
        .map_err(|error| TriggerError::BadRef(error.to_string()))?;

    let sha = match sha {
        Some(sha) => sha,
        None => resolve_ref(&repo.clone_url, &git_ref)
            .await
            .map_err(TriggerError::ResolveFailed)?,
    };

    crate::workspace::checkout::validate_sha(&sha)
        .map_err(|error| TriggerError::BadSha(error.to_string()))?;

    let new = NewRun {
        repo_id: repo.id.clone(),
        trigger: Trigger::Manual,
        git_ref,
        sha,
        message: None,
        // A manual run has no delivery to be a duplicate of; asking for the
        // same commit twice on purpose is allowed.
        delivery_id: None,
    };

    let enqueued = queue::enqueue(db, &new).await?;
    Ok(enqueued.run().clone())
}

/// Registered inside the `/repos` scope rather than here: actix matches scopes
/// by prefix and stops at the first that matches, so a `/repos/...` route
/// declared beside that scope is never reached.
#[post("/{repo_id}/runs")]
pub async fn trigger(
    path: web::Path<String>,
    body: Option<web::Json<TriggerRun>>,
    db: web::Data<Db>,
) -> impl Responder {
    let repo_id = path.into_inner();
    let body = body.map(web::Json::into_inner);

    match trigger_manual(
        &db,
        &repo_id,
        body.as_ref().and_then(|body| body.git_ref.clone()),
        body.as_ref().and_then(|body| body.sha.clone()),
    )
    .await
    {
        Ok(run) => HttpResponse::Accepted().json(run),
        Err(error) => ApiError::from(error).into_response(),
    }
}

#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    pub repo_id: Option<String>,
    #[serde(default)]
    pub limit: Option<i64>,
}

#[get("/runs")]
pub async fn list(query: web::Query<ListQuery>, db: web::Data<Db>) -> impl Responder {
    match queue::list_runs(&db, query.repo_id.as_deref(), query.limit.unwrap_or(50)).await {
        Ok(runs) => HttpResponse::Ok().json(runs),
        Err(error) => ApiError::from(error).into_response(),
    }
}

#[derive(Serialize)]
struct RunDetail {
    #[serde(flatten)]
    run: crate::domain::Run,
    jobs: Vec<crate::domain::Job>,
    artifacts: Vec<crate::domain::Artifact>,
}

#[get("/runs/{id}")]
pub async fn read(path: web::Path<String>, db: web::Data<Db>) -> impl Responder {
    let run = match queue::read_run(&db, &path).await {
        Ok(Some(run)) => run,
        Ok(None) => return json_error(StatusCode::NOT_FOUND, "no such run"),
        Err(error) => return ApiError::from(error).into_response(),
    };

    let jobs = match queue::list_jobs(&db, &run.id).await {
        Ok(jobs) => jobs,
        Err(error) => return ApiError::from(error).into_response(),
    };

    match queue::list_artifacts(&db, &run.id).await {
        Ok(artifacts) => HttpResponse::Ok().json(RunDetail {
            run,
            jobs,
            artifacts,
        }),
        Err(error) => ApiError::from(error).into_response(),
    }
}

#[get("/jobs/{id}/logs")]
pub async fn logs(path: web::Path<String>, db: web::Data<Db>) -> impl Responder {
    // Reads what has been persisted, which is written when a job finishes.
    // Live output for a job still going is phase 8's streaming endpoint.
    match queue::read_logs(&db, &path, -1).await {
        Ok(chunks) => HttpResponse::Ok().json(chunks),
        Err(error) => ApiError::from(error).into_response(),
    }
}

/// Asks a run to stop.
///
/// Accepted rather than OK: whichever replica holds the run notices on its next
/// poll, so the run is still winding down when this returns.
#[post("/runs/{id}/cancel")]
pub async fn cancel(path: web::Path<String>, db: web::Data<Db>) -> impl Responder {
    match queue::request_cancel(&db, &path).await {
        Ok(true) => HttpResponse::Accepted().finish(),
        Ok(false) => json_error(
            StatusCode::CONFLICT,
            "this run is not queued or running; there is nothing to cancel",
        ),
        Err(error) => ApiError::from(error).into_response(),
    }
}

/// Asks the remote what a ref currently points at, without cloning it.
async fn resolve_ref(clone_url: &str, git_ref: &str) -> Result<String, String> {
    let output = tokio::process::Command::new("git")
        .args(["ls-remote", "--exit-code", clone_url, git_ref])
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_ASKPASS", "")
        .env("SSH_ASKPASS", "")
        .kill_on_drop(true)
        .output()
        .await
        .map_err(|error| format!("could not run git: {error}"))?;

    if !output.status.success() {
        // `--exit-code` reports "no such ref" by exiting 2 with nothing on
        // stderr, so the bare message would be "could not resolve X: ".
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = stderr.trim();
        return Err(if detail.is_empty() {
            format!("{git_ref} does not exist in that repository")
        } else {
            format!("could not resolve {git_ref}: {detail}")
        });
    }

    String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .next()
        .map(str::to_string)
        .ok_or_else(|| format!("{git_ref} does not exist in that repository"))
}

pub fn scope() -> actix_web::Scope {
    web::scope("")
        .service(list)
        .service(read)
        .service(logs)
        .service(super::stream::stream_logs)
        .service(cancel)
}
