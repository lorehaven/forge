//! "Run with parameters": a manual run limited to the stages you tick.
//!
//! A plain page rather than something on the home table, which re-renders every few seconds and would
//! throw a half-filled form away. Like the repository edit form it is POST + redirect, gated by the same
//! project-scoped write grant the API uses.

use crate::domain::Repo;
use crate::domain::Trigger;
use crate::routers::api::authz::can_on_project_claims;
use crate::routers::api::runs::{TriggerError, trigger_run};
use crate::routers::ui::common::{ActorOrRedirect, render_page, ui_path};
use crate::scheduler::repos;
use crate::scheduler::stages::{self, KnownStage};
use quench_db::prelude::Db;
use quench_http::prelude::{Bytes, Inject, Path, Query, Response, get, http::StatusCode, post};
use quench_serde::Deserialize;
use quench_web::prelude::*;
use quench_web_components::containers::empty_state;

#[derive(Debug, Default, Deserialize)]
pub(super) struct Notice {
    /// A known error slug: `none` (nothing selected), `failed`, `forbidden`.
    #[serde(default)]
    err: Option<String>,
}

fn form_path(repo: &Repo) -> String {
    format!(
        "/repos/{}/{}/run",
        quench_serde::urlencoded::percent_encode(&repo.owner),
        quench_serde::urlencoded::percent_encode(&repo.name)
    )
}

/// The little control that takes you to the form, for a repository's row or page.
#[must_use]
pub fn launch_link(repo: &Repo) -> Element {
    a().class("log-action")
        .attr("href", ui_path(&form_path(repo)))
        .attr("title", "Run with parameters")
        .attr("data-i18n-title", "ui_launch_link")
        .child(i().class("fas").class("fa-sliders"))
}

fn error_key(slug: &str) -> Option<&'static str> {
    match slug {
        "none" => Some("ui_launch_err_none"),
        "failed" => Some("ui_launch_err_failed"),
        "forbidden" => Some("ui_launch_no_access"),
        _ => None,
    }
}

/// The stage checkboxes, then a field for names the list does not know yet.
#[must_use]
pub fn stage_fields(known: &[KnownStage]) -> Element {
    let mut fields = div().class("launch-stages");
    if known.is_empty() {
        fields = fields.child(
            p().class("repos-hint")
                .attr("data-i18n", "ui_launch_none_known"),
        );
    }
    for (index, stage) in known.iter().enumerate() {
        let id = format!("stage-{index}");
        let mut checkbox = checkbox()
            .attr("id", &id)
            .attr("name", "stage")
            .attr("value", &stage.name);
        if stage.on_by_default {
            checkbox = checkbox.attr("checked", "checked");
        }
        fields = fields.child(
            div()
                .class("launch-stage")
                .child(checkbox)
                .child(label().attr("for", &id).text(&stage.name))
                .child_opt((!stage.on_by_default).then(|| {
                    span()
                        .class("muted")
                        .attr("data-i18n", "ui_launch_normally_skipped")
                })),
        );
    }
    fields
}

/// The whole form for `repo`.
#[must_use]
pub fn launch_form(repo: &Repo, known: &[KnownStage], can_run: bool) -> Element {
    let mut run_form = form()
        .attr("method", "post")
        .attr("action", ui_path(&form_path(repo)))
        .child(
            label()
                .attr("for", "git_ref")
                .attr("data-i18n", "ui_launch_ref"),
        )
        .child(
            input()
                .attr("type", "text")
                .attr("id", "git_ref")
                .attr("name", "git_ref")
                .attr("placeholder", &repo.default_branch),
        )
        .child(h3().attr("data-i18n", "ui_launch_stages"))
        .child(stage_fields(known))
        .child(
            label()
                .attr("for", "extra")
                .attr("data-i18n", "ui_launch_extra"),
        )
        .child(
            input()
                .attr("type", "text")
                .attr("id", "extra")
                .attr("name", "extra")
                .attr("placeholder", "security, lint"),
        );
    let mut submit = button()
        .attr("type", "submit")
        .attr("data-i18n", "ui_launch_submit");
    if !can_run {
        submit = submit.attr("disabled", "disabled");
        run_form = run_form.child(
            p().class("repos-hint")
                .attr("data-i18n", "ui_launch_no_access"),
        );
    }
    run_form.child(submit)
}

async fn load_repo(db: &Db, owner: &str, name: &str) -> Option<Repo> {
    repos::find_by_owner_name(db, owner, name).await.ok()?
}

#[get("/ui/repos/{owner}/{name}/run")]
pub(super) async fn launch_page(
    actor: ActorOrRedirect,
    Path((owner, name)): Path<(String, String)>,
    Query(notice): Query<Notice>,
    Inject(db): Inject<Db>,
) -> Response {
    let claims = match actor.or_redirect() {
        Ok(claims) => claims,
        Err(response) => return response,
    };
    let Some(repo) = load_repo(&db, &owner, &name).await else {
        return render_page(
            StatusCode::NOT_FOUND,
            content().class("repos-content").child(
                div()
                    .class("repos-container")
                    .child(empty_state("ui_scan_repo_not_found")),
            ),
        );
    };

    let known = stages::known(&db, &repo.id).await.unwrap_or_default();
    let can_run = can_on_project_claims(&claims, &db, &repo.project_id, "write").await;

    render_page(
        StatusCode::OK,
        content().class("repos-content").child(
            div()
                .class("repos-container")
                .child(
                    a().class("button repos-back")
                        .attr(
                            "href",
                            ui_path(&format!("/repos/{}/{}/scan", repo.owner, repo.name)),
                        )
                        .attr("data-i18n", "ui_scan_back"),
                )
                .child_opt(notice.err.as_deref().and_then(error_key).map(|key| {
                    p().class("repos-hint finding-severity-error")
                        .attr("data-i18n", key)
                }))
                .child(
                    div()
                        .class("panel repos-panel")
                        .child(
                            div()
                                .class("panel-title")
                                .text(format!("{}/{}", repo.owner, repo.name)),
                        )
                        .child(
                            p().class("repos-hint")
                                .attr("data-i18n", "ui_launch_subtitle"),
                        )
                        .child(
                            div()
                                .class("meta-list")
                                .child(launch_form(&repo, &known, can_run)),
                        ),
                ),
        ),
    )
}

fn redirect(path: &str) -> Response {
    Response::new(StatusCode::FOUND).header("Location", ui_path(path))
}

/// The stages a submitted form asked for: every ticked box, plus the comma-separated extras.
#[must_use]
pub fn selected_stages(pairs: &[(String, String)]) -> Vec<String> {
    let mut chosen: Vec<String> = Vec::new();
    for (key, value) in pairs {
        let names: Vec<&str> = match key.as_str() {
            "stage" => vec![value.as_str()],
            "extra" => value.split(',').collect(),
            _ => continue,
        };
        for name in names.into_iter().map(str::trim).filter(|n| !n.is_empty()) {
            if !chosen.iter().any(|c| c == name) {
                chosen.push(name.to_string());
            }
        }
    }
    chosen
}

#[post("/ui/repos/{owner}/{name}/run")]
pub(super) async fn launch(
    actor: ActorOrRedirect,
    Path((owner, name)): Path<(String, String)>,
    Bytes(body): Bytes,
    Inject(db): Inject<Db>,
) -> Response {
    let claims = match actor.or_redirect() {
        Ok(claims) => claims,
        Err(response) => return response,
    };
    let Some(repo) = load_repo(&db, &owner, &name).await else {
        return redirect("/repos");
    };
    let back = |err: &str| redirect(&format!("{}?err={err}", form_path(&repo)));

    if !can_on_project_claims(&claims, &db, &repo.project_id, "write").await {
        return back("forbidden");
    }

    let pairs = quench_serde::urlencoded::parse_pairs(&String::from_utf8_lossy(&body));
    let chosen = selected_stages(&pairs);
    if chosen.is_empty() {
        return back("none");
    }
    let git_ref = pairs
        .iter()
        .find(|(key, _)| key == "git_ref")
        .map(|(_, value)| value.trim().to_string())
        .filter(|value| !value.is_empty())
        // A bare branch name is what people type; the run records the full ref.
        .map(|value| {
            if value.starts_with("refs/") {
                value
            } else {
                format!("refs/heads/{value}")
            }
        });

    match trigger_run(
        &db,
        &repo.id,
        git_ref,
        None,
        Trigger::Manual,
        None,
        Some(chosen),
    )
    .await
    {
        Ok(enqueued) => redirect(&format!("/runs/{}", enqueued.run().id)),
        Err(TriggerError::BadStages(_)) => back("none"),
        Err(error) => {
            quench_log::warn!(
                "run with parameters for {}/{} could not start: {error}",
                owner,
                name
            );
            back("failed")
        }
    }
}

pub(super) fn register_routes() {
    let _ = launch_page as fn(_, _, _, _) -> _;
    let _ = launch as fn(_, _, _, _) -> _;
}
