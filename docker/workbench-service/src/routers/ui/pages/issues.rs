//! An issue's detail page: editable fields plus its comment thread.

use crate::domain::comment::{self, NewComment};
use crate::domain::issue::{self, IssueUpdate, STATUSES};
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
                        ),
                )
                .child(
                    button()
                        .attr("type", "submit")
                        .attr("data-i18n", "ui_issue_save_button"),
                ),
        )
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

    let changes = IssueUpdate {
        title: form.title.trim().to_string(),
        description: (!form.description.trim().is_empty())
            .then(|| form.description.trim().to_string()),
        kind: form.kind.clone(),
        priority: form.priority.clone(),
        assignee: (!form.assignee.trim().is_empty()).then(|| form.assignee.trim().to_string()),
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
