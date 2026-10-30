//! What the browser client reads beyond the plain lists.
//!
//! Who may do what, the front page's runs, the paged history, the checks of every repository, one repository's scan, and the
//! credentials the caller may see.
//!
//! The htmx pages computed all of this from the database inside the page; the client has no database, so each is a read here,
//! with the same rule: what the caller may not read is left out, not refused.

use crate::config::ConveyorConfig;
use crate::credentials::store as credential_store;
use crate::domain::{Project, Repo, Run};
use crate::routers::api::authz::{can_on_project, granted_project_ids};
use crate::routers::api::{Actor, ApiError, OptionalClaims, json_error};
use crate::scan::{self, CheckKind, CheckResult, ScanSummary};
use crate::scheduler::{projects, queue, repos};
use conveyor_core::format::page_count;
use conveyor_core::{
    Access, CheckChip, CheckDetail, Finding, RepoChecks, RunsPage, ScanFinding, ScanFindings,
    ScanReport,
};
use quench_auth::domain::jwt::{Claims, JwtConfig};
use quench_core::bytes::Bytes;
use quench_core::future::join_all;
use quench_db::prelude::Db;
use quench_http::prelude::{Inject, Path, Query, Response, get, http::StatusCode};
use quench_serde::Deserialize;
use std::collections::{HashMap, HashSet};

/// Pulled before the per-repo cap applies: generous enough that a quiet repo is not crowded off by a noisy one.
const FETCH: i64 = 200;

fn ok<T: quench_serde::Serialize>(value: &T) -> Response {
    Response::json(StatusCode::OK, value)
        .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
}

/// Newest `max_per_repo` runs per repo, capped at `max_total` total. `runs` must already be newest first.
#[must_use]
pub fn cap_per_repo(runs: &[Run], max_total: usize, max_per_repo: usize) -> Vec<Run> {
    let mut seen: HashMap<&str, usize> = HashMap::new();
    let mut selected = Vec::new();

    for run in runs {
        if selected.len() >= max_total {
            break;
        }
        let count = seen.entry(run.repo_id.as_str()).or_insert(0);
        if *count >= max_per_repo {
            continue;
        }
        *count += 1;
        selected.push(run.clone());
    }

    selected
}

/// `root_id` and every project nested under it, however deep.
#[must_use]
pub fn descendant_project_ids(root_id: &str, all_projects: &[Project]) -> HashSet<String> {
    let mut children_of: HashMap<&str, Vec<&Project>> = HashMap::new();
    for project in all_projects {
        if let Some(parent_id) = &project.parent_id {
            children_of
                .entry(parent_id.as_str())
                .or_default()
                .push(project);
        }
    }

    let mut ids = HashSet::new();
    let mut stack = vec![root_id];
    while let Some(id) = stack.pop() {
        if !ids.insert(id.to_string()) {
            continue;
        }
        if let Some(children) = children_of.get(id) {
            stack.extend(children.iter().map(|child| child.id.as_str()));
        }
    }

    ids
}

/// The ids of the repositories under `project_ids`.
#[must_use]
pub fn repo_ids_under<S: std::hash::BuildHasher>(
    project_ids: &HashSet<String, S>,
    repositories: &[Repo],
) -> Vec<String> {
    repositories
        .iter()
        .filter(|repo| project_ids.contains(&repo.project_id))
        .map(|repo| repo.id.clone())
        .collect()
}

/// The projects `claims` may `action` on: `None` when it is all of them (auth off, or the blanket grant).
async fn reachable(
    claims: Option<&Claims>,
    db: &Db,
    action: &str,
) -> Result<Option<HashSet<String>>, ApiError> {
    let Some(claims) = claims else {
        return Ok(None);
    };
    if claims.can("conveyor", action) {
        return Ok(None);
    }
    let granted = granted_project_ids(claims, action);
    let ids = projects::descendant_ids(db, &granted).await?;
    Ok(Some(ids.into_iter().collect()))
}

/// Every project the caller may read or write.
#[get("/api/v1/access")]
pub async fn access(
    Actor(username): Actor,
    OptionalClaims(claims): OptionalClaims,
    Inject(db): Inject<Db>,
) -> Response {
    let all = match projects::list_all(&db).await {
        Ok(all) => all,
        Err(error) => return ApiError::from(error).into_response(),
    };
    let ids = |allowed: Option<HashSet<String>>| -> Vec<String> {
        all.iter()
            .filter(|p| allowed.as_ref().is_none_or(|set| set.contains(&p.id)))
            .map(|p| p.id.clone())
            .collect()
    };
    let read = match reachable(claims.as_ref(), &db, "read").await {
        Ok(set) => ids(set),
        Err(error) => return error.into_response(),
    };
    let write = match reachable(claims.as_ref(), &db, "write").await {
        Ok(set) => ids(set),
        Err(error) => return error.into_response(),
    };
    ok(&Access {
        username,
        read,
        write,
    })
}

/// The repositories the caller may read.
async fn visible_repos(claims: Option<&Claims>, db: &Db) -> Result<Vec<Repo>, ApiError> {
    let all = repos::list(db).await?;
    Ok(match reachable(claims, db, "read").await? {
        None => all,
        Some(set) => all
            .into_iter()
            .filter(|repo| set.contains(&repo.project_id))
            .collect(),
    })
}

#[derive(Debug, Deserialize)]
pub struct ScopeQuery {
    #[serde(default)]
    pub project: Option<String>,
    #[serde(default)]
    pub page: Option<u32>,
}

/// The repositories a page of runs is limited to: `None` is "every repository" (nothing to filter by).
async fn run_scope(
    claims: Option<&Claims>,
    db: &Db,
    project: Option<&str>,
) -> Result<Option<Vec<String>>, ApiError> {
    let restricted = reachable(claims, db, "read").await?;
    let visible = visible_repos(claims, db).await?;
    let Some(project) = project else {
        return Ok(restricted.map(|_| visible.into_iter().map(|r| r.id).collect()));
    };
    let all = projects::list_all(db).await?;
    if !all.iter().any(|p| p.id == project) {
        return Err(ApiError::new(StatusCode::NOT_FOUND, "no such project"));
    }
    let under = descendant_project_ids(project, &all);
    Ok(Some(repo_ids_under(&under, &visible)))
}

/// The front page's runs: the newest, capped per repository and in total by the configuration.
#[get("/api/v1/overview/runs")]
pub async fn recent_runs(
    OptionalClaims(claims): OptionalClaims,
    Query(query): Query<ScopeQuery>,
    Inject(config): Inject<ConveyorConfig>,
    Inject(db): Inject<Db>,
) -> Response {
    let scope = match run_scope(claims.as_ref(), &db, query.project.as_deref()).await {
        Ok(scope) => scope,
        Err(error) => return error.into_response(),
    };
    match queue::list_runs_page(&db, scope.as_deref(), FETCH, 0).await {
        Ok(runs) => ok(&cap_per_repo(
            &runs,
            config.home_recent_runs,
            config.home_max_runs_per_repo,
        )),
        Err(error) => ApiError::from(error).into_response(),
    }
}

/// One page of the full history, newest first.
#[get("/api/v1/overview/history")]
pub async fn history(
    OptionalClaims(claims): OptionalClaims,
    Query(query): Query<ScopeQuery>,
    Inject(config): Inject<ConveyorConfig>,
    Inject(db): Inject<Db>,
) -> Response {
    let scope = match run_scope(claims.as_ref(), &db, query.project.as_deref()).await {
        Ok(scope) => scope,
        Err(error) => return error.into_response(),
    };
    let size = i64::try_from(config.runs_page_size).unwrap_or(25);
    let page = query.page.unwrap_or(1).max(1);
    let offset = i64::from(page - 1) * size;

    let total = match queue::count_runs(&db, scope.as_deref()).await {
        Ok(total) => total,
        Err(error) => return ApiError::from(error).into_response(),
    };
    match queue::list_runs_page(&db, scope.as_deref(), size, offset).await {
        Ok(runs) => ok(&RunsPage {
            runs,
            page,
            pages: page_count(total, size),
            total,
        }),
        Err(error) => ApiError::from(error).into_response(),
    }
}

fn chip(check: &CheckResult) -> CheckChip {
    CheckChip {
        kind: check.kind.slug().to_string(),
        passed: check.passed,
        findings: check.findings.len(),
        metric: check.metric.clone(),
    }
}

/// The check chips of every repository the caller may read.
#[get("/api/v1/scan")]
pub async fn chips(OptionalClaims(claims): OptionalClaims, Inject(db): Inject<Db>) -> Response {
    let visible = match visible_repos(claims.as_ref(), &db).await {
        Ok(visible) => visible,
        Err(error) => return error.into_response(),
    };
    // Run concurrently: `scan::latest` is several sequential round trips on its own.
    let fetches = visible.iter().map(|repo| {
        let db = &db;
        async move {
            let summary = scan::latest(db, &repo.id).await.unwrap_or_default();
            RepoChecks {
                repo_id: repo.id.clone(),
                checks: ScanSummary::KINDS
                    .iter()
                    .filter_map(|kind| summary.get(*kind))
                    .map(chip)
                    .collect(),
            }
        }
    });
    ok(&join_all(fetches).await)
}

/// The repository, if the caller may read it.
async fn readable_repo(
    claims: Option<&Claims>,
    config: &JwtConfig,
    db: &Db,
    id: &str,
) -> Result<Repo, Response> {
    let repo = match repos::read(db, id).await {
        Ok(Some(repo)) => repo,
        Ok(None) => return Err(json_error(StatusCode::NOT_FOUND, "no such repository")),
        Err(error) => return Err(ApiError::from(error).into_response()),
    };
    if !can_on_project(claims, config, db, &repo.project_id, "read").await {
        return Err(json_error(
            StatusCode::FORBIDDEN,
            "no read access to this repository",
        ));
    }
    Ok(repo)
}

fn detail(check: &CheckResult, summary: &ScanSummary) -> CheckDetail {
    CheckDetail {
        kind: check.kind.slug().to_string(),
        job_name: check.job_name.clone(),
        passed: check.passed,
        headline: check.headline.clone(),
        findings: check
            .findings
            .iter()
            .map(|f| Finding {
                title: f.title.clone(),
                id: f.id.clone(),
                severity: f.severity.clone(),
                date: f.date.clone(),
                location: f.location.clone(),
                extra: f.extra.clone(),
            })
            .collect(),
        metric: check.metric.clone(),
        has_report: check.report_uri.is_some(),
        origin: summary.origin(check.kind).cloned(),
    }
}

/// A repository's code-quality summary.
#[get("/api/v1/repos/{id}/scan")]
pub async fn repo_scan(
    OptionalClaims(claims): OptionalClaims,
    Path(id): Path<String>,
    Inject(db): Inject<Db>,
    Inject(config): Inject<JwtConfig>,
) -> Response {
    let repo = match readable_repo(claims.as_ref(), &config, &db, &id).await {
        Ok(repo) => repo,
        Err(response) => return response,
    };
    match scan::latest(&db, &repo.id).await {
        Ok(summary) => ok(&ScanReport {
            run: summary.run.clone(),
            checks: ScanSummary::KINDS
                .iter()
                .filter_map(|kind| summary.get(*kind))
                .map(|check| detail(check, &summary))
                .collect(),
        }),
        Err(error) => ApiError::from(error).into_response(),
    }
}

/// Every finding of the newest security scan, at every severity.
#[get("/api/v1/repos/{id}/scan/findings")]
pub async fn scanner_findings(
    OptionalClaims(claims): OptionalClaims,
    Path(id): Path<String>,
    Inject(db): Inject<Db>,
    Inject(config): Inject<JwtConfig>,
) -> Response {
    let repo = match readable_repo(claims.as_ref(), &config, &db, &id).await {
        Ok(repo) => repo,
        Err(response) => return response,
    };
    let Some((found, artifact)) = scan::latest_findings(&db, &repo.id).await else {
        return json_error(StatusCode::NOT_FOUND, "no scanner findings");
    };
    ok(&ScanFindings {
        findings: found
            .into_iter()
            .map(|f| ScanFinding {
                severity: f.severity,
                check: f.check,
                service: f.service,
                method: f.method,
                path: f.path,
                detail: f.detail,
            })
            .collect(),
        run_id: artifact.run_id,
        created_at: artifact.created_at,
    })
}

#[derive(Debug, Default, Deserialize)]
pub struct ReportQuery {
    /// Any value: hand the page over as a file to save, instead of showing it.
    pub download: Option<String>,
}

/// The stored report page of a check that keeps one (tests, security).
///
/// Served by conveyor: warehouse hands files out as
/// downloads, and it needs the reader to hold a warehouse grant. Sandboxed (`allow-scripts` only, no origin of its own) so a
/// report can filter itself but never reach conveyor's session.
#[get("/api/v1/repos/{id}/scan/{category}/report")]
pub async fn check_report(
    OptionalClaims(claims): OptionalClaims,
    Path((id, category)): Path<(String, String)>,
    Query(query): Query<ReportQuery>,
    Inject(db): Inject<Db>,
    Inject(config): Inject<JwtConfig>,
) -> Response {
    let Some(kind @ (CheckKind::Tests | CheckKind::Security)) = CheckKind::from_slug(&category)
    else {
        return json_error(StatusCode::NOT_FOUND, "no such report");
    };
    let repo = match readable_repo(claims.as_ref(), &config, &db, &id).await {
        Ok(repo) => repo,
        Err(response) => return response,
    };
    let summary = match scan::latest(&db, &repo.id).await {
        Ok(summary) => summary,
        Err(error) => return ApiError::from(error).into_response(),
    };
    let (Some(uri), Some(store)) = (
        summary.get(kind).and_then(|check| check.report_uri.clone()),
        scan::warehouse_store(),
    ) else {
        return json_error(StatusCode::NOT_FOUND, "this check has no report");
    };

    let page = match store.download(&uri).await {
        Ok(page) => page,
        Err(error) => {
            quench_log::warn!(
                "could not read the {category} report of {}: {error}",
                repo.slug()
            );
            return Response::text(StatusCode::BAD_GATEWAY, "the report could not be read");
        }
    };

    let response = Response::from_bytes(StatusCode::OK, Bytes::from(page))
        .header("content-type", "text/html; charset=utf-8")
        .header("content-security-policy", "sandbox allow-scripts")
        .header("x-content-type-options", "nosniff")
        .header("cache-control", "private, max-age=60");
    if query.download.is_some() {
        response.header(
            "content-disposition",
            format!(
                "attachment; filename=\"{}-{}-{category}-report.html\"",
                repo.owner, repo.name
            ),
        )
    } else {
        response
    }
}

/// The credentials the caller may read, never their tokens. A credential of a repository is as readable as the repository.
#[get("/api/v1/credentials")]
pub async fn credentials(
    OptionalClaims(claims): OptionalClaims,
    Inject(db): Inject<Db>,
) -> Response {
    let readable = match reachable(claims.as_ref(), &db, "read").await {
        Ok(set) => set,
        Err(error) => return error.into_response(),
    };
    let all_repos = match repos::list(&db).await {
        Ok(all) => all,
        Err(error) => return ApiError::from(error).into_response(),
    };
    let project_of: HashMap<&str, &str> = all_repos
        .iter()
        .map(|repo| (repo.id.as_str(), repo.project_id.as_str()))
        .collect();
    let all = match credential_store::list_all(&db).await {
        Ok(all) => all,
        Err(error) => {
            quench_log::warn!("could not list the credentials: {error}");
            return json_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "could not list credentials",
            );
        }
    };
    let shown: Vec<_> = all
        .into_iter()
        .filter(|credential| {
            let project = credential.project_id.as_deref().or_else(|| {
                credential
                    .repo_id
                    .as_deref()
                    .and_then(|repo| project_of.get(repo).copied())
            });
            // A repo since removed has no project: left out rather than guessed at.
            project.is_some_and(|p| readable.as_ref().is_none_or(|set| set.contains(p)))
        })
        .collect();
    ok(&shown)
}

pub fn register_routes() {
    let _ = access as fn(_, _, _) -> _;
    let _ = recent_runs as fn(_, _, _, _) -> _;
    let _ = history as fn(_, _, _, _) -> _;
    let _ = chips as fn(_, _) -> _;
    let _ = repo_scan as fn(_, _, _, _) -> _;
    let _ = scanner_findings as fn(_, _, _, _) -> _;
    let _ = check_report as fn(_, _, _, _, _) -> _;
    let _ = credentials as fn(_, _) -> _;
}
