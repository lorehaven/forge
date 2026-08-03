//! Registering, editing and removing repositories from a browser, instead of
//! only through the JSON API.
//!
//! Plain forms with a POST and a redirect - the same shape gatehouse's
//! `/admin/users` pages use, and for the same reason: this is a mutation
//! consequential enough that it should not depend on JavaScript to submit,
//! and conveyor's UI already leans on plain navigation everywhere else (see
//! `scan.rs`).
//!
//! Unlike the rest of conveyor's UI - which shows every project and
//! repository to any signed-in visitor - these pages enforce the same
//! project-scoped write grants the JSON API does
//! (`routers::api::authz::can_on_project_claims`). Read-only browsing stays
//! unscoped (a visitor can always land on `/repos` or an edit page from a
//! link), but nothing here is submittable without a write grant on the
//! repository's project.

use crate::domain::{Project, Provider, Repo};
use crate::routers::api::authz::{can_on_project_claims, granted_project_ids};
use crate::routers::ui::common::{UiPageKind, render_page, ui_login_redirect_for, ui_path};
use crate::scheduler::repos::{NewRepo, RepoUpdate};
use crate::scheduler::{projects, repos};
use actix_web::{HttpRequest, HttpResponse, Responder, get, post, web};
use quench_auth::actix::routers::ui::get_user_from_req;
use quench_auth::prelude::{Claims, JwtConfig};
use quench_db::prelude::Db;
use quench_web::prelude::*;
use serde::Deserialize;
use std::collections::HashSet;

/// `Element` has no conditional attribute setter, and `selected`/`checked`/
/// `disabled` are exactly the attributes that want one - see gatehouse's
/// `admin.rs`, where this same trait was introduced first.
trait AttrIf {
    fn attr_if(self, condition: bool, key: &str, value: &str) -> Self;
}

impl AttrIf for Element {
    fn attr_if(self, condition: bool, key: &str, value: &str) -> Self {
        if condition {
            self.attr(key, value)
        } else {
            self
        }
    }
}

/// Feedback carried across the redirect that follows every write.
#[derive(Deserialize, Default)]
pub struct Notice {
    /// A known error slug, validated before it reaches the page - see
    /// `known_error_key`.
    #[serde(default)]
    pub err: Option<String>,
    #[serde(default)]
    pub ok: Option<String>,
}

async fn actor(request: &HttpRequest, config: &JwtConfig) -> Option<Claims> {
    get_user_from_req(request, config).await
}

/// The project ids `claims` may write to: every project when they hold the
/// blanket `conveyor:write` grant (or auth is disabled - `get_user_from_req`
/// already folds that into a synthetic all-access `Claims`), otherwise
/// whatever they hold a resource-scoped grant on plus everything nested
/// beneath it.
async fn writable_project_ids(db: &Db, claims: &Claims, all_projects: &[Project]) -> Vec<String> {
    if claims.can("conveyor", "write") {
        return all_projects.iter().map(|p| p.id.clone()).collect();
    }
    let granted = granted_project_ids(claims, "write");
    projects::descendant_ids(db, &granted).await.unwrap_or_default()
}

/// `root/.../leaf`, read out of an in-memory project list rather than a
/// per-repository query - the same tree `home.rs` already holds in memory to
/// render its own panel.
pub fn project_path(id: &str, all_projects: &[Project]) -> String {
    let mut names = Vec::new();
    let mut current = all_projects.iter().find(|p| p.id == id);
    while let Some(project) = current {
        names.push(project.name.as_str());
        current = project
            .parent_id
            .as_deref()
            .and_then(|parent_id| all_projects.iter().find(|p| p.id == parent_id));
    }
    names.reverse();
    names.join("/")
}

// ---------------------------------------------------------------------------
// Pages
// ---------------------------------------------------------------------------

#[get("/repos")]
pub(super) async fn list_page(
    request: HttpRequest,
    config: web::Data<JwtConfig>,
    db: web::Data<Db>,
    notice: web::Query<Notice>,
) -> impl Responder {
    let Some(claims) = actor(&request, &config).await else {
        return ui_login_redirect_for(&request);
    };
    render_list(&db, &claims, &notice).await
}

#[get("/repos/{owner}/{name}/edit")]
pub(super) async fn edit_page(
    request: HttpRequest,
    path: web::Path<(String, String)>,
    config: web::Data<JwtConfig>,
    db: web::Data<Db>,
    notice: web::Query<Notice>,
) -> impl Responder {
    let Some(claims) = actor(&request, &config).await else {
        return ui_login_redirect_for(&request);
    };
    let (owner, name) = path.into_inner();
    match repos::find_by_owner_name(&db, &owner, &name).await {
        Ok(Some(repo)) => render_edit(&db, &repo, &claims, &notice).await,
        _ => not_found(),
    }
}

// ---------------------------------------------------------------------------
// Forms
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub(super) struct CreateForm {
    pub owner: String,
    pub name: String,
    pub clone_url: String,
    #[serde(default)]
    pub default_branch: Option<String>,
    pub project_id: String,
    #[serde(default)]
    pub provider: Option<String>,
}

#[post("/repos")]
pub(super) async fn create_repo(
    request: HttpRequest,
    form: web::Form<CreateForm>,
    config: web::Data<JwtConfig>,
    db: web::Data<Db>,
) -> impl Responder {
    let Some(claims) = actor(&request, &config).await else {
        return ui_login_redirect_for(&request);
    };
    let form = form.into_inner();

    if form.owner.trim().is_empty() || form.name.trim().is_empty() {
        return redirect_to_list(Some("owner_name_empty"));
    }
    if form.project_id.trim().is_empty() {
        return redirect_to_list(Some("project_required"));
    }
    if crate::workspace::checkout::validate_url(&form.clone_url).is_err() {
        return redirect_to_list(Some("bad_clone_url"));
    }
    if !can_on_project_claims(&claims, &db, &form.project_id, "write").await {
        return redirect_to_list(Some("forbidden"));
    }

    let provider = form
        .provider
        .as_deref()
        .and_then(Provider::parse)
        .unwrap_or(Provider::GitHub);

    let new = NewRepo {
        provider,
        owner: form.owner.trim().to_string(),
        name: form.name.trim().to_string(),
        clone_url: form.clone_url.trim().to_string(),
        default_branch: form
            .default_branch
            .clone()
            .unwrap_or_else(|| "master".to_string()),
        registered_by: claims.sub.clone(),
        project_id: form.project_id.clone(),
    };

    match repos::create(&db, &new).await {
        Ok(_) => redirect("/repos?ok=created"),
        Err(_) => redirect_to_list(Some("create_failed")),
    }
}

#[derive(Deserialize)]
pub(super) struct EditForm {
    pub owner: String,
    pub name: String,
    pub clone_url: String,
    #[serde(default)]
    pub default_branch: Option<String>,
    pub project_id: String,
    /// Absent when the checkbox was left unchecked - a browser omits an
    /// unchecked box from the submission entirely, it does not send `false`.
    #[serde(default)]
    pub enabled: Option<String>,
}

#[post("/repos/{owner}/{name}/edit")]
pub(super) async fn save_repo(
    request: HttpRequest,
    path: web::Path<(String, String)>,
    form: web::Form<EditForm>,
    config: web::Data<JwtConfig>,
    db: web::Data<Db>,
) -> impl Responder {
    let Some(claims) = actor(&request, &config).await else {
        return ui_login_redirect_for(&request);
    };
    let (owner, name) = path.into_inner();
    let form = form.into_inner();

    let repo = match repos::find_by_owner_name(&db, &owner, &name).await {
        Ok(Some(repo)) => repo,
        _ => return redirect("/repos?err=not_found"),
    };

    if !can_on_project_claims(&claims, &db, &repo.project_id, "write").await {
        return redirect_to_edit(&owner, &name, Some("forbidden"));
    }
    if form.owner.trim().is_empty() || form.name.trim().is_empty() {
        return redirect_to_edit(&owner, &name, Some("owner_name_empty"));
    }
    if form.project_id.trim().is_empty() {
        return redirect_to_edit(&owner, &name, Some("project_required"));
    }
    if crate::workspace::checkout::validate_url(&form.clone_url).is_err() {
        return redirect_to_edit(&owner, &name, Some("bad_clone_url"));
    }
    // Moving a repository to a different project needs write on both ends -
    // otherwise a write grant on one project alone would let it pull a
    // repository in from a project the caller has no access to.
    if form.project_id != repo.project_id
        && !can_on_project_claims(&claims, &db, &form.project_id, "write").await
    {
        return redirect_to_edit(&owner, &name, Some("forbidden"));
    }

    let changes = RepoUpdate {
        owner: form.owner.trim().to_string(),
        name: form.name.trim().to_string(),
        clone_url: form.clone_url.trim().to_string(),
        default_branch: form
            .default_branch
            .clone()
            .unwrap_or_else(|| "master".to_string()),
        project_id: form.project_id.clone(),
        enabled: form.enabled.is_some(),
    };

    match repos::update(&db, &repo.id, &changes).await {
        Ok(Some(updated)) => redirect(&format!(
            "/repos/{}/{}/edit?ok=saved",
            urlencoding::encode(&updated.owner),
            urlencoding::encode(&updated.name)
        )),
        Ok(None) => redirect("/repos?err=not_found"),
        Err(_) => redirect_to_edit(&owner, &name, Some("save_failed")),
    }
}

#[post("/repos/{owner}/{name}/delete")]
pub(super) async fn delete_repo(
    request: HttpRequest,
    path: web::Path<(String, String)>,
    config: web::Data<JwtConfig>,
    db: web::Data<Db>,
) -> impl Responder {
    let Some(claims) = actor(&request, &config).await else {
        return ui_login_redirect_for(&request);
    };
    let (owner, name) = path.into_inner();

    let repo = match repos::find_by_owner_name(&db, &owner, &name).await {
        Ok(Some(repo)) => repo,
        _ => return redirect("/repos?err=not_found"),
    };

    if !can_on_project_claims(&claims, &db, &repo.project_id, "write").await {
        return redirect_to_edit(&owner, &name, Some("forbidden"));
    }

    match repos::delete(&db, &repo.id).await {
        Ok(true) => redirect("/repos?ok=deleted"),
        Ok(false) => redirect("/repos?err=not_found"),
        Err(_) => redirect_to_edit(&owner, &name, Some("delete_failed")),
    }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

async fn render_list(db: &Db, claims: &Claims, notice: &Notice) -> HttpResponse {
    let all_projects = projects::list_all(db).await.unwrap_or_default();
    let repositories = repos::list(db).await.unwrap_or_default();
    let writable = writable_project_ids(db, claims, &all_projects).await;
    let writable_set: HashSet<&str> = writable.iter().map(String::as_str).collect();
    let writable_projects: Vec<&Project> = all_projects
        .iter()
        .filter(|project| writable_set.contains(project.id.as_str()))
        .collect();

    let mut rows = div().class("meta-list");
    if repositories.is_empty() {
        rows = rows.child(div().class("empty").attr("data-i18n", "ui_repos_empty"));
    }
    for repo in &repositories {
        rows = rows.child(repo_row(
            repo,
            &project_path(&repo.project_id, &all_projects),
            writable_set.contains(repo.project_id.as_str()),
        ));
    }

    let list_panel = div()
        .class("panel repos-panel")
        .child(
            div()
                .class("panel-title")
                .attr("data-i18n", "ui_repos_title"),
        )
        .child(rows);

    render_page(
        HttpResponse::Ok(),
        content().class("repos-content").child(
            div()
                .class("repos-container")
                .child_opt(notice_banner(notice))
                .child(
                    a().class("button repos-back")
                        .attr("href", ui_path("/home"))
                        .attr("data-i18n", "ui_repos_back_home"),
                )
                .child(list_panel)
                .child_opt(create_panel(&writable_projects, &all_projects)),
        ),
        UiPageKind::Home,
    )
}

pub fn repo_row(repo: &Repo, project_path: &str, can_edit: bool) -> Element {
    let mut row = div()
        .class("repos-row")
        .child(
            div()
                .class("repos-row-main")
                .child(
                    span()
                        .class("repos-repo-name")
                        .text(format!("{}/{}", repo.owner, repo.name)),
                )
                .child(span().class("repos-project-path").text(project_path)),
        )
        .child(
            div()
                .class("repos-row-meta")
                .child(span().class("muted").text(repo.provider.to_string()))
                .child(span().class("mono").text(&repo.default_branch))
                .child(if repo.enabled {
                    span()
                        .class("status status-success")
                        .attr("data-i18n", "ui_repo_enabled")
                } else {
                    span()
                        .class("status status-skipped")
                        .attr("data-i18n", "ui_repo_disabled")
                }),
        );

    if can_edit {
        row = row.child(
            a().class("button repos-edit")
                .attr(
                    "href",
                    ui_path(&format!(
                        "/repos/{}/{}/edit",
                        urlencoding::encode(&repo.owner),
                        urlencoding::encode(&repo.name)
                    )),
                )
                .attr("data-i18n", "ui_repos_edit"),
        );
    }

    row
}

/// Omitted, not disabled, when there is nowhere the caller may register a
/// repository - there is nothing honest to disable a form control into when
/// the whole action is out of reach.
pub fn create_panel(writable_projects: &[&Project], all_projects: &[Project]) -> Option<Element> {
    if writable_projects.is_empty() {
        return None;
    }

    let mut project_select = select().attr("id", "new-project").attr("name", "project_id");
    for project in writable_projects {
        project_select = project_select.child(
            option()
                .attr("value", &project.id)
                .text(project_path(&project.id, all_projects)),
        );
    }

    let mut provider_select = select().attr("id", "new-provider").attr("name", "provider");
    for (value, label) in [("github", "GitHub"), ("generic", "Generic")] {
        provider_select = provider_select.child(option().attr("value", value).text(label));
    }

    let create_form = form()
        .attr("method", "post")
        .attr("action", ui_path("/repos"))
        .child(
            label()
                .attr("for", "new-owner")
                .attr("data-i18n", "ui_repos_owner"),
        )
        .child(
            input()
                .attr("type", "text")
                .attr("id", "new-owner")
                .attr("name", "owner")
                .attr("required", "required"),
        )
        .child(
            label()
                .attr("for", "new-name")
                .attr("data-i18n", "ui_repos_name"),
        )
        .child(
            input()
                .attr("type", "text")
                .attr("id", "new-name")
                .attr("name", "name")
                .attr("required", "required"),
        )
        .child(
            label()
                .attr("for", "new-clone-url")
                .attr("data-i18n", "ui_repos_clone_url"),
        )
        .child(
            input()
                .attr("type", "text")
                .attr("id", "new-clone-url")
                .attr("name", "clone_url")
                .attr("required", "required"),
        )
        .child(
            label()
                .attr("for", "new-branch")
                .attr("data-i18n", "ui_repos_branch"),
        )
        .child(
            input()
                .attr("type", "text")
                .attr("id", "new-branch")
                .attr("name", "default_branch")
                .attr("placeholder", "master"),
        )
        .child(
            label()
                .attr("for", "new-project")
                .attr("data-i18n", "ui_repos_project"),
        )
        .child(project_select)
        .child(
            label()
                .attr("for", "new-provider")
                .attr("data-i18n", "ui_repos_provider"),
        )
        .child(provider_select)
        .child(
            button()
                .attr("type", "submit")
                .attr("data-i18n", "ui_repos_create"),
        );

    Some(
        div()
            .class("panel repos-panel")
            .child(
                div()
                    .class("panel-title")
                    .attr("data-i18n", "ui_repos_add_title"),
            )
            .child(div().class("meta-list").child(create_form)),
    )
}

async fn render_edit(db: &Db, repo: &Repo, claims: &Claims, notice: &Notice) -> HttpResponse {
    let all_projects = projects::list_all(db).await.unwrap_or_default();
    let writable = writable_project_ids(db, claims, &all_projects).await;
    let writable_set: HashSet<&str> = writable.iter().map(String::as_str).collect();
    let can_edit = writable_set.contains(repo.project_id.as_str());
    let writable_projects: Vec<&Project> = all_projects
        .iter()
        .filter(|project| writable_set.contains(project.id.as_str()))
        .collect();

    let fields = edit_fields(repo, &all_projects, &writable_projects, !can_edit);

    let panel_body = if can_edit {
        let edit_form = form()
            .attr("method", "post")
            .attr(
                "action",
                ui_path(&format!(
                    "/repos/{}/{}/edit",
                    urlencoding::encode(&repo.owner),
                    urlencoding::encode(&repo.name)
                )),
            )
            .child(fields)
            .child(
                button()
                    .attr("type", "submit")
                    .attr("data-i18n", "ui_repos_save"),
            );

        div()
            .class("panel repos-panel")
            .child(
                div()
                    .class("panel-title")
                    .text(format!("{}/{}", repo.owner, repo.name)),
            )
            .child(div().class("meta-list").child(edit_form))
    } else {
        div()
            .class("panel repos-panel")
            .child(
                div()
                    .class("panel-title")
                    .text(format!("{}/{}", repo.owner, repo.name)),
            )
            .child(
                div()
                    .class("meta-list")
                    .child(
                        p().class("repos-hint")
                            .attr("data-i18n", "ui_repos_view_only"),
                    )
                    .child(fields),
            )
    };

    let danger = can_edit.then(|| {
        div()
            .class("panel repos-panel repos-danger")
            .child(
                div()
                    .class("panel-title")
                    .attr("data-i18n", "ui_repos_delete_title"),
            )
            .child(
                div().class("meta-list").child(
                    form()
                        .attr("method", "post")
                        .attr(
                            "action",
                            ui_path(&format!(
                                "/repos/{}/{}/delete",
                                urlencoding::encode(&repo.owner),
                                urlencoding::encode(&repo.name)
                            )),
                        )
                        .child(
                            p().class("repos-hint")
                                .attr("data-i18n", "ui_repos_delete_hint"),
                        )
                        .child(
                            button()
                                .attr("type", "submit")
                                .class("repos-delete")
                                .attr("data-i18n", "ui_repos_delete"),
                        ),
                ),
            )
    });

    render_page(
        HttpResponse::Ok(),
        content().class("repos-content").child(
            div()
                .class("repos-container")
                .child_opt(notice_banner(notice))
                .child(
                    a().class("button repos-back")
                        .attr("href", ui_path("/repos"))
                        .attr("data-i18n", "ui_repos_back"),
                )
                .child(panel_body)
                .child_opt(danger),
        ),
        UiPageKind::Home,
    )
}

/// The field list shared by the editable form and the read-only view: same
/// controls either way, `disabled` just decides whether a submission could
/// ever reach the server that already re-checks every one of these.
pub fn edit_fields(
    repo: &Repo,
    all_projects: &[Project],
    writable_projects: &[&Project],
    disabled: bool,
) -> Element {
    let mut project_select = select().attr("id", "project_id").attr("name", "project_id");
    if disabled {
        project_select = project_select.attr("disabled", "disabled").child(
            option()
                .attr("value", &repo.project_id)
                .attr("selected", "selected")
                .text(project_path(&repo.project_id, all_projects)),
        );
    } else {
        for project in writable_projects {
            project_select = project_select.child(
                option()
                    .attr("value", &project.id)
                    .text(project_path(&project.id, all_projects))
                    .attr_if(project.id == repo.project_id, "selected", "selected"),
            );
        }
    }

    let mut enabled_box = checkbox()
        .attr("id", "enabled")
        .attr("name", "enabled")
        .attr_if(repo.enabled, "checked", "checked");
    if disabled {
        enabled_box = enabled_box.attr("disabled", "disabled");
    }

    div()
        .child(
            label()
                .attr("for", "owner")
                .attr("data-i18n", "ui_repos_owner"),
        )
        .child(
            input()
                .attr("type", "text")
                .attr("id", "owner")
                .attr("name", "owner")
                .attr("value", &repo.owner)
                .attr("required", "required")
                .attr_if(disabled, "disabled", "disabled"),
        )
        .child(
            label()
                .attr("for", "name")
                .attr("data-i18n", "ui_repos_name"),
        )
        .child(
            input()
                .attr("type", "text")
                .attr("id", "name")
                .attr("name", "name")
                .attr("value", &repo.name)
                .attr("required", "required")
                .attr_if(disabled, "disabled", "disabled"),
        )
        .child(
            label()
                .attr("for", "clone_url")
                .attr("data-i18n", "ui_repos_clone_url"),
        )
        .child(
            input()
                .attr("type", "text")
                .attr("id", "clone_url")
                .attr("name", "clone_url")
                .attr("value", &repo.clone_url)
                .attr("required", "required")
                .attr_if(disabled, "disabled", "disabled"),
        )
        .child(
            label()
                .attr("for", "default_branch")
                .attr("data-i18n", "ui_repos_branch"),
        )
        .child(
            input()
                .attr("type", "text")
                .attr("id", "default_branch")
                .attr("name", "default_branch")
                .attr("value", &repo.default_branch)
                .attr_if(disabled, "disabled", "disabled"),
        )
        .child(
            label()
                .attr("for", "project_id")
                .attr("data-i18n", "ui_repos_project"),
        )
        .child(project_select)
        .child(
            div()
                .class("repos-checkbox-row")
                .child(enabled_box)
                .child(
                    label()
                        .attr("for", "enabled")
                        .attr("data-i18n", "ui_repo_enabled"),
                ),
        )
}

fn not_found() -> HttpResponse {
    render_page(
        HttpResponse::NotFound(),
        content().class("repos-content").child(
            div().class("repos-container").child(
                div()
                    .class("empty")
                    .attr("data-i18n", "ui_repos_not_found"),
            ),
        ),
        UiPageKind::Home,
    )
}

fn redirect(path: &str) -> HttpResponse {
    HttpResponse::Found()
        .append_header(("Location", ui_path(path)))
        .finish()
}

fn redirect_to_list(err: Option<&str>) -> HttpResponse {
    match err {
        Some(key) => redirect(&format!("/repos?err={key}")),
        None => redirect("/repos"),
    }
}

fn redirect_to_edit(owner: &str, name: &str, err: Option<&str>) -> HttpResponse {
    let base = format!(
        "/repos/{}/{}/edit",
        urlencoding::encode(owner),
        urlencoding::encode(name)
    );
    match err {
        Some(key) => redirect(&format!("{base}?err={key}")),
        None => redirect(&base),
    }
}

/// Success or failure from the write that redirected here. The error slug is
/// checked against a fixed allowlist rather than trusted from the query
/// string, so a hand-crafted link cannot put arbitrary text on the page.
pub fn notice_banner(notice: &Notice) -> Option<Element> {
    if let Some(key) = notice.err.as_deref().and_then(known_error_key) {
        return Some(p().class("repos-notice error").attr("data-i18n", key));
    }
    let key = match notice.ok.as_deref() {
        Some("created") => "ui_repos_ok_created",
        Some("saved") => "ui_repos_ok_saved",
        Some("deleted") => "ui_repos_ok_deleted",
        _ => return None,
    };
    Some(p().class("repos-notice ok").attr("data-i18n", key))
}

pub fn known_error_key(candidate: &str) -> Option<&'static str> {
    match candidate {
        "owner_name_empty" => Some("ui_repos_err_owner_name_empty"),
        "project_required" => Some("ui_repos_err_project_required"),
        "bad_clone_url" => Some("ui_repos_err_bad_clone_url"),
        "forbidden" => Some("ui_repos_err_forbidden"),
        "not_found" => Some("ui_repos_err_not_found"),
        "create_failed" | "save_failed" | "delete_failed" => Some("ui_repos_err_write_failed"),
        _ => None,
    }
}
