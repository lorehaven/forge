//! Triggering runs, and looking at what they did.

use crate::credentials::store::{self as credential_store, ResolvedCredential};
use crate::domain::{Repo, Run, Trigger};
use crate::routers::api::authz::{can_on_project, granted_project_ids};
use crate::routers::api::{ApiError, OptionalClaims, json_error};
use crate::scheduler::queue::{self, NewRun, QueueError};
use crate::scheduler::{projects, repos};
use crate::secrets::crypto::SecretKey;
use crate::workspace::checkout::{basic_auth_header, credential_env};
use quench_auth::domain::jwt::{Claims, JwtConfig};
use quench_db::prelude::Db;
use quench_http::prelude::{
    Bytes, FromRequest, HttpError, Inject, Path, Query, Request, Response, get, http::StatusCode,
    post,
};
use serde::{Deserialize, Serialize};

/// `None` when there's no such repository, so "forbidden" can be told from "not found".
async fn repo_access(
    claims: Option<&Claims>,
    config: &JwtConfig,
    db: &Db,
    repo_id: &str,
    action: &str,
) -> Result<Option<bool>, QueueError> {
    match repos::read(db, repo_id).await? {
        Some(repo) => Ok(Some(
            can_on_project(claims, config, db, &repo.project_id, action).await,
        )),
        None => Ok(None),
    }
}

#[derive(Deserialize, Default)]
pub struct TriggerRun {
    /// Branch or tag to build. Defaults to the repository's default branch.
    #[serde(default)]
    pub git_ref: Option<String>,
    /// The commit to build; without it conveyor asks the repository what the ref currently points at.
    #[serde(default)]
    pub sha: Option<String>,
}

/// `TriggerRun`, but an absent/empty body means "every field default" - `Json<T>` alone rejects zero bytes as invalid JSON.
pub struct MaybeTriggerRun(pub TriggerRun);

#[async_trait::async_trait]
impl FromRequest for MaybeTriggerRun {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
        let Bytes(bytes) = Bytes::from_request(req).await?;
        if bytes.is_empty() {
            return Ok(Self(TriggerRun::default()));
        }
        serde_json::from_slice(&bytes)
            .map(Self)
            .map_err(|e| quench_http::prelude::ExtractError::InvalidJson(e.to_string()).into())
    }
}

/// Why [`trigger_manual`] could not start a run - shared between the JSON API and the UI's "run now" button.
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
            return ApiError::new(status, error.to_string());
        };
        Self::from(queue_error)
    }
}

/// Starts a run by hand - the pipeline's `on` patterns don't gate this. Without a `sha`, the ref's
/// current commit is resolved once here so the run records what it actually built.
pub async fn trigger_manual(
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
        None => {
            // Same credential a checkout would use - this talks to the remote directly, not via `workspace::checkout`.
            let credential = resolve_credential(db, &repo).await;
            let header = credential
                .as_ref()
                .map(|c| basic_auth_header(&c.username, &c.token));
            let extra_env = credential_env(header.as_deref());

            resolve_ref(&repo.clone_url, &git_ref, &extra_env)
                .await
                .map_err(TriggerError::ResolveFailed)?
        }
    };

    crate::workspace::checkout::validate_sha(&sha)
        .map_err(|error| TriggerError::BadSha(error.to_string()))?;

    let new = NewRun {
        repo_id: repo.id.clone(),
        trigger: Trigger::Manual,
        git_ref,
        sha,
        message: None,
        // No delivery to be a duplicate of; asking for the same commit twice on purpose is allowed.
        delivery_id: None,
        resumed_from: None,
    };

    let enqueued = queue::enqueue(db, &new).await?;
    Ok(enqueued.run().clone())
}

#[post("/api/v1/repos/{repo_id}/runs")]
pub async fn trigger(
    OptionalClaims(claims): OptionalClaims,
    Path(repo_id): Path<String>,
    MaybeTriggerRun(body): MaybeTriggerRun,
    Inject(db): Inject<Db>,
    Inject(config): Inject<JwtConfig>,
) -> Response {
    match repo_access(claims.as_ref(), &config, &db, &repo_id, "write").await {
        Ok(Some(true)) => {}
        Ok(Some(false)) => {
            return json_error(StatusCode::FORBIDDEN, "no write access to this repository");
        }
        Ok(None) => return json_error(StatusCode::NOT_FOUND, "no such repository"),
        Err(error) => return ApiError::from(error).into_response(),
    }

    match trigger_manual(&db, &repo_id, body.git_ref, body.sha).await {
        Ok(run) => Response::json(StatusCode::ACCEPTED, &run)
            .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR)),
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

#[get("/api/v1/runs")]
pub async fn list(
    OptionalClaims(claims): OptionalClaims,
    Query(query): Query<ListQuery>,
    Inject(db): Inject<Db>,
    Inject(config): Inject<JwtConfig>,
) -> Response {
    if let Some(repo_id) = &query.repo_id {
        match repo_access(claims.as_ref(), &config, &db, repo_id, "read").await {
            Ok(Some(true)) => {}
            Ok(Some(false)) => {
                return json_error(StatusCode::FORBIDDEN, "no read access to this repository");
            }
            Ok(None) => return json_error(StatusCode::NOT_FOUND, "no such repository"),
            Err(error) => return ApiError::from(error).into_response(),
        }

        return match queue::list_runs(&db, Some(repo_id.as_str()), query.limit.unwrap_or(50)).await
        {
            Ok(runs) => Response::json(StatusCode::OK, &runs)
                .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR)),
            Err(error) => ApiError::from(error).into_response(),
        };
    }

    let all = match queue::list_runs(&db, None, query.limit.unwrap_or(50)).await {
        Ok(runs) => runs,
        Err(error) => return ApiError::from(error).into_response(),
    };

    // No `repo_id`: same directory-listing philosophy as `repos::list`.
    let Some(claims) = claims else {
        return Response::json(StatusCode::OK, &all)
            .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR));
    };
    if claims.can("conveyor", "read") {
        return Response::json(StatusCode::OK, &all)
            .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR));
    }

    let granted = granted_project_ids(&claims, "read");
    let visible_projects = match projects::descendant_ids(&db, &granted).await {
        Ok(ids) => ids,
        Err(error) => return ApiError::from(error).into_response(),
    };
    let visible_repos = match repos::list(&db).await {
        Ok(repos) => repos,
        Err(error) => return ApiError::from(error).into_response(),
    };
    let visible_repo_ids: std::collections::HashSet<String> = visible_repos
        .into_iter()
        .filter(|repo| visible_projects.contains(&repo.project_id))
        .map(|repo| repo.id)
        .collect();

    let runs: Vec<_> = all
        .into_iter()
        .filter(|run| visible_repo_ids.contains(&run.repo_id))
        .collect();
    Response::json(StatusCode::OK, &runs)
        .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
}

#[derive(Serialize)]
struct RunDetail {
    #[serde(flatten)]
    run: crate::domain::Run,
    jobs: Vec<crate::domain::Job>,
    artifacts: Vec<crate::domain::Artifact>,
}

#[get("/api/v1/runs/{id}")]
pub async fn read(
    OptionalClaims(claims): OptionalClaims,
    Path(id): Path<String>,
    Inject(db): Inject<Db>,
    Inject(config): Inject<JwtConfig>,
) -> Response {
    let run = match queue::read_run(&db, &id).await {
        Ok(Some(run)) => run,
        Ok(None) => return json_error(StatusCode::NOT_FOUND, "no such run"),
        Err(error) => return ApiError::from(error).into_response(),
    };

    match repo_access(claims.as_ref(), &config, &db, &run.repo_id, "read").await {
        Ok(Some(true)) => {}
        Ok(Some(false)) => {
            return json_error(StatusCode::FORBIDDEN, "no read access to this run");
        }
        // Repo gone but run row survives (no cascading delete) - treat as "no read access", not 404.
        Ok(None) => return json_error(StatusCode::FORBIDDEN, "no read access to this run"),
        Err(error) => return ApiError::from(error).into_response(),
    }

    let jobs = match queue::list_jobs(&db, &run.id).await {
        Ok(jobs) => jobs,
        Err(error) => return ApiError::from(error).into_response(),
    };

    match queue::list_artifacts(&db, &run.id).await {
        Ok(artifacts) => Response::json(
            StatusCode::OK,
            &RunDetail {
                run,
                jobs,
                artifacts,
            },
        )
        .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR)),
        Err(error) => ApiError::from(error).into_response(),
    }
}

#[get("/api/v1/jobs/{id}/logs")]
pub async fn logs(
    OptionalClaims(claims): OptionalClaims,
    Path(id): Path<String>,
    Inject(db): Inject<Db>,
    Inject(config): Inject<JwtConfig>,
) -> Response {
    match job_access(claims.as_ref(), &config, &db, &id, "read").await {
        Ok(Some(true)) => {}
        Ok(Some(false)) => {
            return json_error(StatusCode::FORBIDDEN, "no read access to this job's logs");
        }
        Ok(None) => return json_error(StatusCode::NOT_FOUND, "no such job"),
        Err(error) => return ApiError::from(error).into_response(),
    }

    // Persisted logs only, written when a job finishes; live output is the streaming endpoint.
    match queue::read_logs(&db, &id, -1).await {
        Ok(chunks) => Response::json(StatusCode::OK, &chunks)
            .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR)),
        Err(error) => ApiError::from(error).into_response(),
    }
}

/// `None` when there is no such job.
async fn job_access(
    claims: Option<&Claims>,
    config: &JwtConfig,
    db: &Db,
    job_id: &str,
    action: &str,
) -> Result<Option<bool>, QueueError> {
    match queue::repo_id_for_job(db, job_id).await? {
        Some(repo_id) => repo_access(claims, config, db, &repo_id, action).await,
        None => Ok(None),
    }
}

/// Accepted, not OK - whichever replica holds the run notices on its next poll; still winding down on return.
#[post("/api/v1/runs/{id}/cancel")]
pub async fn cancel(
    OptionalClaims(claims): OptionalClaims,
    Path(id): Path<String>,
    Inject(db): Inject<Db>,
    Inject(config): Inject<JwtConfig>,
) -> Response {
    let run = match queue::read_run(&db, &id).await {
        Ok(Some(run)) => run,
        Ok(None) => return json_error(StatusCode::NOT_FOUND, "no such run"),
        Err(error) => return ApiError::from(error).into_response(),
    };

    match repo_access(claims.as_ref(), &config, &db, &run.repo_id, "write").await {
        Ok(Some(true)) => {}
        Ok(Some(false)) => {
            return json_error(StatusCode::FORBIDDEN, "no write access to this run");
        }
        Ok(None) => return json_error(StatusCode::FORBIDDEN, "no write access to this run"),
        Err(error) => return ApiError::from(error).into_response(),
    }

    match queue::request_cancel(&db, &id).await {
        Ok(true) => Response::new(StatusCode::ACCEPTED),
        Ok(false) => json_error(
            StatusCode::CONFLICT,
            "this run is not queued or running; there is nothing to cancel",
        ),
        Err(error) => ApiError::from(error).into_response(),
    }
}

/// Why [`restart_run`] could not start a new run.
#[derive(Debug, thiserror::Error)]
pub enum RestartError {
    #[error("no such run")]
    NotFound,
    #[error("only a failed or cancelled run can be restarted")]
    NotRestartable,
    #[error(transparent)]
    Queue(#[from] QueueError),
}

impl From<RestartError> for ApiError {
    fn from(error: RestartError) -> Self {
        let RestartError::Queue(queue_error) = error else {
            let status = match &error {
                RestartError::NotFound => StatusCode::NOT_FOUND,
                RestartError::NotRestartable => StatusCode::CONFLICT,
                RestartError::Queue(_) => unreachable!(),
            };
            return ApiError::new(status, error.to_string());
        };
        Self::from(queue_error)
    }
}

/// A new row, not the old one requeued - `resumed_from` tells the worker to carry over passed stages (`worker::execute_jobs`).
pub async fn restart_run(db: &Db, run_id: &str) -> Result<Run, RestartError> {
    let source = match queue::read_run(db, run_id).await {
        Ok(Some(run)) => run,
        Ok(None) => return Err(RestartError::NotFound),
        Err(error) => return Err(RestartError::Queue(error)),
    };

    if !source.status.is_failure() {
        return Err(RestartError::NotRestartable);
    }

    let new = NewRun {
        repo_id: source.repo_id.clone(),
        trigger: source.trigger,
        git_ref: source.git_ref.clone(),
        sha: source.sha.clone(),
        message: source.message.clone(),
        // Not a redelivery - any number of restarts may follow one failure.
        delivery_id: None,
        resumed_from: Some(source.id.clone()),
    };

    let enqueued = queue::enqueue(db, &new).await?;
    Ok(enqueued.run().clone())
}

/// Restarts a failed or cancelled run. `POST /runs/{id}/cancel`'s sibling:
/// same authorization, same shape, opposite direction.
#[post("/api/v1/runs/{id}/restart")]
pub async fn restart(
    OptionalClaims(claims): OptionalClaims,
    Path(id): Path<String>,
    Inject(db): Inject<Db>,
    Inject(config): Inject<JwtConfig>,
) -> Response {
    let run = match queue::read_run(&db, &id).await {
        Ok(Some(run)) => run,
        Ok(None) => return json_error(StatusCode::NOT_FOUND, "no such run"),
        Err(error) => return ApiError::from(error).into_response(),
    };

    match repo_access(claims.as_ref(), &config, &db, &run.repo_id, "write").await {
        Ok(Some(true)) => {}
        Ok(Some(false)) => {
            return json_error(StatusCode::FORBIDDEN, "no write access to this run");
        }
        Ok(None) => return json_error(StatusCode::FORBIDDEN, "no write access to this run"),
        Err(error) => return ApiError::from(error).into_response(),
    }

    match restart_run(&db, &id).await {
        Ok(run) => Response::json(StatusCode::ACCEPTED, &run)
            .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR)),
        Err(error) => ApiError::from(error).into_response(),
    }
}

/// Same lookup `scheduler::worker`'s checkout does, but resolved fresh here
/// since this runs once per manual trigger, not per queued job.
async fn resolve_credential(db: &Db, repo: &Repo) -> Option<ResolvedCredential> {
    let key = match SecretKey::from_env_named(credential_store::KEY_VAR) {
        Ok(key) => key,
        Err(error) => {
            tracing::warn!("git credentials are unavailable: {error}");
            None
        }
    };
    credential_store::resolve(db, key.as_ref(), repo)
        .await
        .unwrap_or_else(|error| {
            tracing::warn!("could not resolve a credential for {}: {error}", repo.id);
            None
        })
}

/// Asks the remote what a ref currently points at, without cloning it.
async fn resolve_ref(
    clone_url: &str,
    git_ref: &str,
    extra_env: &[(&str, &str)],
) -> Result<String, String> {
    let output = tokio::process::Command::new("git")
        .args(["ls-remote", "--exit-code", clone_url, git_ref])
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_ASKPASS", "")
        .env("SSH_ASKPASS", "")
        .envs(extra_env.iter().copied())
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

pub fn register_routes() {
    let _ = trigger as fn(_, _, _, _, _) -> _;
    let _ = list as fn(_, _, _, _) -> _;
    let _ = read as fn(_, _, _, _) -> _;
    let _ = logs as fn(_, _, _, _) -> _;
    let _ = cancel as fn(_, _, _, _) -> _;
    let _ = restart as fn(_, _, _, _) -> _;
}
