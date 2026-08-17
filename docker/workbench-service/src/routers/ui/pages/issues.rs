//! An issue's detail page: editable fields plus its comment thread.

use crate::domain::comment::{self, NewComment};
use crate::domain::issue::{self, IssueUpdate, STATUSES};
use crate::domain::issue_link::{self, NewIssueLink};
use crate::domain::{project, realm_users};
use crate::routers::api::authz::can_on_project_claims;
use crate::routers::ui::common::{
    Notice, actor, assignee_field, is_ui_authenticated, notice_banner, render_page,
    ui_login_redirect, ui_login_redirect_for, ui_path,
};
use actix_web::{HttpRequest, HttpResponse, Responder, get, post, web};
use quench_auth::prelude::JwtConfig;
use quench_db::prelude::Db;
use quench_web::prelude::*;
use serde::Deserialize;

#[get("/issues/{id}")]
pub(super) async fn detail(
    req: HttpRequest,
    issue_id: web::Path<String>,
    config: web::Data<JwtConfig>,
    db: web::Data<Db>,
    notice: web::Query<Notice>,
) -> impl Responder {
    if !is_ui_authenticated(&req, &config).await {
        return ui_login_redirect();
    }
    let Some(claims) = actor(&req, &config).await else {
        return ui_login_redirect();
    };

    let Some(issue) = issue::read(&db, &issue_id).await.ok().flatten() else {
        return HttpResponse::Found()
            .append_header(("Location", ui_path("/home")))
            .finish();
    };
    let Some(project) = project::read(&db, &issue.project_id).await.ok().flatten() else {
        return HttpResponse::Found()
            .append_header(("Location", ui_path("/home")))
            .finish();
    };
    let comments = comment::list_by_issue(&db, &issue.id)
        .await
        .unwrap_or_default();
    let users = realm_users::list_users(&db).await.unwrap_or_default();
    let related = issue_link::related(&db, &issue.id)
        .await
        .unwrap_or_default();

    render_page(
        HttpResponse::Ok(),
        content().class("home-content").child(
            div()
                .class("home-container")
                .child_opt(notice_banner(&notice))
                .child(
                    div().class("home-header").child(
                        a().attr("href", ui_path(&format!("/projects/{}/board", project.id)))
                            .attr("data-i18n", "ui_board_back"),
                    ),
                )
                .child(edit_panel(&project, &issue, &claims.sub, &users))
                .child(dependencies_panel(&issue.id, &related))
                .child(comments_panel(&issue.id, &comments)),
        ),
    )
}

fn edit_panel(
    project: &project::Project,
    i: &issue::Issue,
    current_user: &str,
    users: &[realm_users::RealmUser],
) -> Element {
    let mut status_select = select().attr("id", "wb-status").attr("name", "status");
    for status in STATUSES {
        let mut opt = option().attr("value", status).text(status);
        if status == i.status {
            opt = opt.attr("selected", "true");
        }
        status_select = status_select.child(opt);
    }

    div()
        .class("panel wb-form-panel")
        .child(
            div()
                .class("panel-title")
                .text(format!("{}-{}", project.key, i.seq)),
        )
        .child(
            form()
                .attr("method", "post")
                .attr("action", ui_path(&format!("/issues/{}", i.id)))
                .class("wb-form")
                .child(
                    label()
                        .attr("for", "wb-title")
                        .attr("data-i18n", "ui_field_title"),
                )
                .child(
                    input()
                        .attr("type", "text")
                        .attr("id", "wb-title")
                        .attr("name", "title")
                        .attr("value", i.title.clone())
                        .attr("required", "true"),
                )
                .child(
                    label()
                        .attr("for", "wb-description")
                        .attr("data-i18n", "ui_field_description"),
                )
                .child(
                    textarea()
                        .attr("id", "wb-description")
                        .attr("name", "description")
                        .text(i.description.clone().unwrap_or_default()),
                )
                .child(
                    div()
                        .class("wb-form-row")
                        .child(
                            div()
                                .child(
                                    label()
                                        .attr("for", "wb-status")
                                        .attr("data-i18n", "ui_field_status"),
                                )
                                .child(status_select),
                        )
                        .child(
                            div()
                                .child(
                                    label()
                                        .attr("for", "wb-kind")
                                        .attr("data-i18n", "ui_field_kind"),
                                )
                                .child(kind_select(&i.kind)),
                        )
                        .child(
                            div()
                                .child(
                                    label()
                                        .attr("for", "wb-priority")
                                        .attr("data-i18n", "ui_field_priority"),
                                )
                                .child(priority_select(&i.priority)),
                        )
                        .child(
                            div()
                                .child(
                                    label()
                                        .attr("for", "wb-assignee")
                                        .attr("data-i18n", "ui_field_assignee"),
                                )
                                .child(assignee_field(current_user, users, i.assignee.as_deref())),
                        )
                        .child(
                            div()
                                .child(
                                    label()
                                        .attr("for", "wb-estimate")
                                        .attr("data-i18n", "ui_field_estimate"),
                                )
                                .child(estimate_field(i.estimate)),
                        ),
                )
                .child(
                    button()
                        .attr("type", "submit")
                        .class("wb-submit")
                        .attr("data-i18n", "ui_issue_save_button"),
                ),
        )
}

/// Story points. A plain number input rather than a `<select>` - unlike
/// status/kind/priority, an estimate has no fixed set of valid values.
fn estimate_field(current: Option<i32>) -> Element {
    let mut el = input()
        .attr("type", "number")
        .attr("id", "wb-estimate")
        .attr("name", "estimate")
        .attr("min", "0")
        .attr("step", "1");
    if let Some(value) = current {
        el = el.attr("value", value.to_string());
    }
    el
}

fn kind_select(current: &str) -> Element {
    let mut el = select().attr("id", "wb-kind").attr("name", "kind");
    for kind in ["task", "bug", "story"] {
        let mut opt = option().attr("value", kind).text(kind);
        if kind == current {
            opt = opt.attr("selected", "true");
        }
        el = el.child(opt);
    }
    el
}

fn priority_select(current: &str) -> Element {
    let mut el = select().attr("id", "wb-priority").attr("name", "priority");
    for priority in ["low", "medium", "high"] {
        let mut opt = option().attr("value", priority).text(priority);
        if priority == current {
            opt = opt.attr("selected", "true");
        }
        el = el.child(opt);
    }
    el
}

/// The three typed-link lists (`blocks`/`blocked_by`/`relates_to`) plus the
/// form that creates a new one, keyed by an issue's displayed `{key}-{seq}`
/// rather than its id - the only identifier a user typing into the field
/// actually knows.
fn dependencies_panel(issue_id: &str, related: &issue_link::RelatedIssues) -> Element {
    div()
        .class("panel wb-form-panel")
        .child(
            div()
                .class("panel-title")
                .attr("data-i18n", "ui_issue_dependencies"),
        )
        .child(link_section(issue_id, "ui_issue_blocks", &related.blocks))
        .child(link_section(
            issue_id,
            "ui_issue_blocked_by",
            &related.blocked_by,
        ))
        .child(link_section(
            issue_id,
            "ui_issue_relates_to",
            &related.relates_to,
        ))
        .child(add_link_form(issue_id))
}

fn link_section(issue_id: &str, title_key: &str, links: &[issue_link::LinkedIssue]) -> Element {
    let mut list = div().class("wb-link-list");
    if links.is_empty() {
        list = list.child(div().class("empty").attr("data-i18n", "ui_issue_no_links"));
    }
    for link in links {
        list = list.child(
            div()
                .class("wb-link-row")
                .child(
                    a().attr("href", ui_path(&format!("/issues/{}", link.issue_id)))
                        .class("wb-link-title")
                        .text(format!(
                            "{}-{} — {}",
                            link.project_key, link.seq, link.title
                        )),
                )
                .child(span().class("wb-link-status").text(link.status.clone()))
                .child(
                    form()
                        .attr("method", "post")
                        .attr(
                            "action",
                            ui_path(&format!("/issues/{issue_id}/links/{}/delete", link.link_id)),
                        )
                        // `display: contents` (see `link_rules` in `common/css.rs`) -
                        // otherwise the form's own block box, not the button
                        // inside it, is what `.wb-link-row`'s flex layout sizes,
                        // and the button falls back to the shared `button`
                        // rule's block-level `display: flex` filling it.
                        .class("wb-link-remove-form")
                        .child(
                            button()
                                .attr("type", "submit")
                                .class("wb-link-remove")
                                .attr("title", "Remove")
                                .text("×"),
                        ),
                ),
        );
    }

    div()
        .class("wb-link-section")
        .child(
            div()
                .class("wb-link-section-title")
                .attr("data-i18n", title_key),
        )
        .child(list)
}

fn add_link_form(issue_id: &str) -> Element {
    form()
        .attr("method", "post")
        .attr("action", ui_path(&format!("/issues/{issue_id}/links")))
        .class("wb-form")
        .child(
            div()
                .class("wb-form-row")
                .child(
                    div()
                        .child(
                            label()
                                .attr("for", "wb-link-target")
                                .attr("data-i18n", "ui_field_link_target"),
                        )
                        .child(
                            input()
                                .attr("type", "text")
                                .attr("id", "wb-link-target")
                                .attr("name", "target_key")
                                .attr("placeholder", "e.g. WB-4")
                                .attr("required", "true"),
                        ),
                )
                .child(
                    div()
                        .child(
                            label()
                                .attr("for", "wb-link-kind")
                                .attr("data-i18n", "ui_field_link_kind"),
                        )
                        .child(link_kind_select()),
                ),
        )
        .child(
            button()
                .attr("type", "submit")
                .class("wb-submit")
                .attr("data-i18n", "ui_issue_add_link_button"),
        )
}

fn link_kind_select() -> Element {
    let mut el = select().attr("id", "wb-link-kind").attr("name", "kind");
    for (value, label_text) in [("blocks", "blocks"), ("relates_to", "relates to")] {
        el = el.child(option().attr("value", value).text(label_text));
    }
    el
}

fn comments_panel(issue_id: &str, comments: &[comment::Comment]) -> Element {
    let mut list = div().class("meta-list");
    if comments.is_empty() {
        list = list.child(
            div()
                .class("empty")
                .attr("data-i18n", "ui_issue_empty_comments"),
        );
    }
    for c in comments {
        list = list.child(
            div()
                .class("wb-card")
                .child(
                    div()
                        .class("wb-card-meta")
                        .child(strong().text(c.author.clone()))
                        .child(span().text(c.created_at.to_rfc3339())),
                )
                .child(p().text(c.body.clone())),
        );
    }

    div()
        .class("panel wb-form-panel")
        .child(
            div()
                .class("panel-title")
                .attr("data-i18n", "ui_issue_comments"),
        )
        .child(list)
        .child(
            form()
                .attr("method", "post")
                .attr("action", ui_path(&format!("/issues/{issue_id}/comments")))
                .class("wb-form")
                .child(
                    label()
                        .attr("for", "wb-comment-body")
                        .attr("data-i18n", "ui_field_comment_body"),
                )
                .child(
                    textarea()
                        .attr("id", "wb-comment-body")
                        .attr("name", "body")
                        .attr("required", "true"),
                )
                .child(
                    button()
                        .attr("type", "submit")
                        .class("wb-submit")
                        .attr("data-i18n", "ui_issue_add_comment_button"),
                ),
        )
}

#[derive(Deserialize)]
pub(super) struct UpdateIssueForm {
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub kind: String,
    pub priority: String,
    #[serde(default)]
    pub assignee: String,
    pub status: String,
    #[serde(default)]
    pub estimate: String,
}

#[post("/issues/{id}")]
pub(super) async fn update(
    request: HttpRequest,
    issue_id: web::Path<String>,
    config: web::Data<JwtConfig>,
    db: web::Data<Db>,
    form: web::Form<UpdateIssueForm>,
) -> impl Responder {
    let Some(claims) = actor(&request, &config).await else {
        return ui_login_redirect_for(&request);
    };

    let Some(issue) = issue::read(&db, &issue_id).await.ok().flatten() else {
        return redirect_home(Some("not_found"));
    };

    if form.title.trim().is_empty() {
        return redirect_issue(&issue.id, Some("title_required"));
    }

    if !can_on_project_claims(&claims, &issue.project_id, "write") {
        return redirect_issue(&issue.id, Some("forbidden"));
    }

    let estimate = match form.estimate.trim() {
        "" => None,
        value => match value.parse::<i32>() {
            Ok(value) if value >= 0 => Some(value),
            _ => return redirect_issue(&issue.id, Some("invalid_estimate")),
        },
    };

    let changes = IssueUpdate {
        title: form.title.trim().to_string(),
        description: (!form.description.trim().is_empty())
            .then(|| form.description.trim().to_string()),
        kind: form.kind.clone(),
        priority: form.priority.clone(),
        assignee: (!form.assignee.trim().is_empty()).then(|| form.assignee.trim().to_string()),
        estimate,
    };

    match issue::update(&db, &issue.id, &changes).await {
        Ok(_) => {}
        // The only user-typed field here that's a foreign key is `assignee`.
        Err(error) if error.is_foreign_key_violation() => {
            return redirect_issue(&issue.id, Some("unknown_assignee"));
        }
        Err(_) => return redirect_issue(&issue.id, Some("update_failed")),
    }

    if issue::is_valid_status(&form.status) && form.status != issue.status {
        let _ = issue::transition(&db, &issue.id, &form.status).await;
    }

    redirect_issue(&issue.id, None)
}

#[derive(Deserialize)]
pub(super) struct CreateCommentForm {
    pub body: String,
}

#[post("/issues/{id}/comments")]
pub(super) async fn create_comment(
    request: HttpRequest,
    issue_id: web::Path<String>,
    config: web::Data<JwtConfig>,
    db: web::Data<Db>,
    form: web::Form<CreateCommentForm>,
) -> impl Responder {
    let Some(claims) = actor(&request, &config).await else {
        return ui_login_redirect_for(&request);
    };

    let Some(issue) = issue::read(&db, &issue_id).await.ok().flatten() else {
        return redirect_home(Some("not_found"));
    };

    if form.body.trim().is_empty() {
        return redirect_issue(&issue.id, Some("comment_required"));
    }

    if !can_on_project_claims(&claims, &issue.project_id, "write") {
        return redirect_issue(&issue.id, Some("forbidden"));
    }

    let new = NewComment {
        issue_id: issue.id.clone(),
        author: claims.sub.clone(),
        body: form.body.trim().to_string(),
    };

    match comment::create(&db, &new).await {
        Ok(_) => redirect_issue(&issue.id, None),
        Err(_) => redirect_issue(&issue.id, Some("comment_failed")),
    }
}

#[derive(Deserialize)]
pub(super) struct AddLinkForm {
    /// The issue's displayed key (`WB-4`), not its id - the form typing this
    /// in has no reason to know the id.
    pub target_key: String,
    pub kind: String,
}

#[post("/issues/{id}/links")]
pub(super) async fn add_link(
    request: HttpRequest,
    issue_id: web::Path<String>,
    config: web::Data<JwtConfig>,
    db: web::Data<Db>,
    form: web::Form<AddLinkForm>,
) -> impl Responder {
    let Some(claims) = actor(&request, &config).await else {
        return ui_login_redirect_for(&request);
    };

    let Some(issue) = issue::read(&db, &issue_id).await.ok().flatten() else {
        return redirect_home(Some("not_found"));
    };

    if !can_on_project_claims(&claims, &issue.project_id, "write") {
        return redirect_issue(&issue.id, Some("forbidden"));
    }

    if !issue_link::is_valid_kind(&form.kind) {
        return redirect_issue(&issue.id, Some("invalid_link_kind"));
    }

    let Some((project_key, seq)) = split_issue_key(&form.target_key) else {
        return redirect_issue(&issue.id, Some("invalid_issue_key"));
    };
    let Some(target_project) = project::read_by_key(&db, &project_key).await.ok().flatten() else {
        return redirect_issue(&issue.id, Some("unknown_issue_key"));
    };
    let Some(target) = issue::read_by_seq(&db, &target_project.id, seq)
        .await
        .ok()
        .flatten()
    else {
        return redirect_issue(&issue.id, Some("unknown_issue_key"));
    };

    if target.id == issue.id {
        return redirect_issue(&issue.id, Some("self_link"));
    }

    let new = NewIssueLink {
        issue_id: issue.id.clone(),
        linked_issue_id: target.id,
        kind: form.kind.clone(),
    };

    match issue_link::create(&db, &new).await {
        Ok(_) => redirect_issue(&issue.id, None),
        Err(error) if error.is_unique_violation() => redirect_issue(&issue.id, Some("link_exists")),
        Err(_) => redirect_issue(&issue.id, Some("link_failed")),
    }
}

/// `"WB-4"` -> `("WB", 4)`. `rsplit_once` rather than `split_once` because a
/// project key itself may not contain `-`, but nothing enforces that except
/// convention - splitting from the right is the one choice that still works
/// if it ever does.
fn split_issue_key(key: &str) -> Option<(String, i32)> {
    let (project_key, seq) = key.trim().rsplit_once('-')?;
    let seq: i32 = seq.trim().parse().ok()?;
    Some((project_key.trim().to_string(), seq))
}

#[post("/issues/{id}/links/{link_id}/delete")]
pub(super) async fn remove_link(
    request: HttpRequest,
    path: web::Path<(String, String)>,
    config: web::Data<JwtConfig>,
    db: web::Data<Db>,
) -> impl Responder {
    let (issue_id, link_id) = path.into_inner();

    let Some(claims) = actor(&request, &config).await else {
        return ui_login_redirect_for(&request);
    };

    let Some(issue) = issue::read(&db, &issue_id).await.ok().flatten() else {
        return redirect_home(Some("not_found"));
    };

    if !can_on_project_claims(&claims, &issue.project_id, "write") {
        return redirect_issue(&issue.id, Some("forbidden"));
    }

    let _ = issue_link::delete(&db, &link_id).await;
    redirect_issue(&issue.id, None)
}

fn redirect_issue(issue_id: &str, error: Option<&str>) -> HttpResponse {
    let base = ui_path(&format!("/issues/{issue_id}"));
    let location = match error {
        Some(code) => format!("{base}?error={code}"),
        None => base,
    };
    HttpResponse::Found()
        .append_header(("Location", location))
        .finish()
}

fn redirect_home(error: Option<&str>) -> HttpResponse {
    let location = match error {
        Some(code) => format!("{}?error={code}", ui_path("/home")),
        None => ui_path("/home"),
    };
    HttpResponse::Found()
        .append_header(("Location", location))
        .finish()
}
