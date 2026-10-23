//! An issue's detail page: editable fields plus its comment thread.

use crate::domain::comment::{self, NewComment};
use crate::domain::issue::{self, IssueUpdate, STATUSES};
use crate::domain::issue_link::{self, NewIssueLink};
use crate::domain::{project, realm_users};
use crate::routers::api::authz::can_on_project_claims;
use crate::routers::ui::common::{
    ActorOrRedirect, Notice, assignee_field, notice_banner, render_page, ui_path,
};
use forge_ui::{
    Card, PageHeader, Tone, Variant, badge, field_for, form_el, icon, select_el, submit_button, t,
    text_input, textarea_el, txt,
};
use quench_db::prelude::Db;
use quench_http::prelude::{Form, Inject, Path, Query, Response, get, http::StatusCode, post};
use quench_serde::Deserialize;
use quench_web::prelude::*;

#[get("/ui/issues/{id}")]
pub(super) async fn detail(
    actor: ActorOrRedirect,
    Path(issue_id): Path<String>,
    Inject(db): Inject<Db>,
    Query(notice): Query<Notice>,
) -> Response {
    let claims = match actor.or_redirect() {
        Ok(claims) => claims,
        Err(response) => return response,
    };

    let Some(issue) = issue::read(&db, &issue_id).await.ok().flatten() else {
        return Response::new(StatusCode::FOUND).header("Location", ui_path("/home"));
    };
    let Some(project) = project::read(&db, &issue.project_id).await.ok().flatten() else {
        return Response::new(StatusCode::FOUND).header("Location", ui_path("/home"));
    };
    let comments = comment::list_by_issue(&db, &issue.id)
        .await
        .unwrap_or_default();
    let users = realm_users::list_users(&db).await.unwrap_or_default();
    let related = issue_link::related(&db, &issue.id)
        .await
        .unwrap_or_default();

    render_page(
        StatusCode::OK,
        div()
            .class("fui-stack fui-stack-lg")
            .child_opt(notice_banner(&notice))
            .child(
                PageHeader::titled(issue.title.clone())
                    .crumb(t("ui_nav_projects"), Some(&ui_path("/home")))
                    .crumb(
                        txt(format!("{} — {}", project.key, project.name)),
                        Some(&ui_path(&format!("/projects/{}/board", project.id))),
                    )
                    .crumb(txt(format!("{}-{}", project.key, issue.seq)), None)
                    .build(),
            )
            .child(edit_panel(&project, &issue, &claims.sub, &users))
            .child(dependencies_panel(&issue.id, &related))
            .child(comments_panel(&issue.id, &comments)),
    )
}

fn edit_panel(
    project: &project::Project,
    i: &issue::Issue,
    current_user: &str,
    users: &[realm_users::RealmUser],
) -> Element {
    let mut status_select = select_el("status").attr("id", "wb-status");
    for status in STATUSES {
        let mut opt = option().attr("value", status).text(status);
        if status == i.status {
            opt = opt.attr("selected", "true");
        }
        status_select = status_select.child(opt);
    }

    let form = form_el("post", &ui_path(&format!("/issues/{}", i.id)))
        .child(field_for(
            "wb-title",
            "ui_field_title",
            text_input("title", "text")
                .attr("id", "wb-title")
                .attr("value", i.title.clone())
                .attr("required", "true"),
            None,
        ))
        .child(field_for(
            "wb-description",
            "ui_field_description",
            textarea_el("description")
                .attr("id", "wb-description")
                .text(i.description.clone().unwrap_or_default()),
            None,
        ))
        .child(
            div()
                .class("wb-form-row")
                .child(field_for(
                    "wb-status",
                    "ui_field_status",
                    status_select,
                    None,
                ))
                .child(field_for(
                    "wb-kind",
                    "ui_field_kind",
                    kind_select(&i.kind),
                    None,
                ))
                .child(field_for(
                    "wb-priority",
                    "ui_field_priority",
                    priority_select(&i.priority),
                    None,
                ))
                .child(field_for(
                    "wb-assignee",
                    "ui_field_assignee",
                    assignee_field(current_user, users, i.assignee.as_deref()),
                    None,
                ))
                .child(field_for(
                    "wb-estimate",
                    "ui_field_estimate",
                    estimate_field(i.estimate),
                    None,
                )),
        )
        .child(
            div()
                .class("fui-form-actions fui-end")
                .child(submit_button(Variant::Primary, "ui_issue_save_button")),
        );

    Card::new()
        .title_key("ui_issue_details")
        .action(badge(Tone::Brand, format!("{}-{}", project.key, i.seq)))
        .body(form)
        .build()
}

/// Story points. A plain number input rather than a `<select>` - unlike
/// status/kind/priority, an estimate has no fixed set of valid values.
fn estimate_field(current: Option<i32>) -> Element {
    let mut el = text_input("estimate", "number")
        .attr("id", "wb-estimate")
        .attr("min", "0")
        .attr("step", "1");
    if let Some(value) = current {
        el = el.attr("value", value.to_string());
    }
    el
}

fn kind_select(current: &str) -> Element {
    let mut el = select_el("kind").attr("id", "wb-kind");
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
    let mut el = select_el("priority").attr("id", "wb-priority");
    for priority in ["low", "medium", "high"] {
        let mut opt = option().attr("value", priority).text(priority);
        if priority == current {
            opt = opt.attr("selected", "true");
        }
        el = el.child(opt);
    }
    el
}

/// The three typed-link lists plus the add-link form.
fn dependencies_panel(issue_id: &str, related: &issue_link::RelatedIssues) -> Element {
    Card::new()
        .title_key("ui_issue_dependencies")
        .body(link_section(issue_id, "ui_issue_blocks", &related.blocks))
        .body(link_section(
            issue_id,
            "ui_issue_blocked_by",
            &related.blocked_by,
        ))
        .body(link_section(
            issue_id,
            "ui_issue_relates_to",
            &related.relates_to,
        ))
        .body(add_link_form(issue_id))
        .build()
}

fn link_section(issue_id: &str, title_key: &str, links: &[issue_link::LinkedIssue]) -> Element {
    let mut list = div().class("wb-link-list");
    if links.is_empty() {
        list = list.child(
            p().class("fui-muted")
                .attr("data-i18n", "ui_issue_no_links"),
        );
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
                .child(badge(Tone::Gray, link.status.clone()))
                .child(
                    form()
                        .attr("method", "post")
                        .attr(
                            "action",
                            ui_path(&format!("/issues/{issue_id}/links/{}/delete", link.link_id)),
                        )
                        // `display: contents` - see `link_rules` in `common/css.rs`.
                        .class("wb-link-remove-form")
                        .child(
                            button()
                                .attr("type", "submit")
                                .class("fui-icon-btn")
                                .attr("title", "Remove")
                                .child(icon("xmark")),
                        ),
                ),
        );
    }

    div()
        .child(
            div()
                .class("wb-link-section-title")
                .attr("data-i18n", title_key),
        )
        .child(list)
}

fn add_link_form(issue_id: &str) -> Element {
    form_el("post", &ui_path(&format!("/issues/{issue_id}/links")))
        .child(
            div()
                .class("wb-form-row")
                .child(field_for(
                    "wb-link-target",
                    "ui_field_link_target",
                    text_input("target_key", "text")
                        .attr("id", "wb-link-target")
                        .attr("placeholder", "e.g. WB-4")
                        .attr("required", "true"),
                    None,
                ))
                .child(field_for(
                    "wb-link-kind",
                    "ui_field_link_kind",
                    link_kind_select(),
                    None,
                )),
        )
        .child(div().class("fui-form-actions fui-end").child(submit_button(
            Variant::Secondary,
            "ui_issue_add_link_button",
        )))
}

fn link_kind_select() -> Element {
    let mut el = select_el("kind").attr("id", "wb-link-kind");
    for (value, label_text) in [("blocks", "blocks"), ("relates_to", "relates to")] {
        el = el.child(option().attr("value", value).text(label_text));
    }
    el
}

fn comments_panel(issue_id: &str, comments: &[comment::Comment]) -> Element {
    let mut list = div().class("fui-stack");
    if comments.is_empty() {
        list = list.child(
            p().class("fui-muted")
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

    let form = form_el("post", &ui_path(&format!("/issues/{issue_id}/comments")))
        .child(field_for(
            "wb-comment-body",
            "ui_field_comment_body",
            textarea_el("body")
                .attr("id", "wb-comment-body")
                .attr("required", "true"),
            None,
        ))
        .child(div().class("fui-form-actions fui-end").child(submit_button(
            Variant::Primary,
            "ui_issue_add_comment_button",
        )));

    Card::new()
        .title_key("ui_issue_comments")
        .body(list)
        .body(form)
        .build()
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

#[post("/ui/issues/{id}")]
pub(super) async fn update(
    actor: ActorOrRedirect,
    Path(issue_id): Path<String>,
    Inject(db): Inject<Db>,
    Form(form): Form<UpdateIssueForm>,
) -> Response {
    let claims = match actor.or_redirect() {
        Ok(claims) => claims,
        Err(response) => return response,
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

#[post("/ui/issues/{id}/comments")]
pub(super) async fn create_comment(
    actor: ActorOrRedirect,
    Path(issue_id): Path<String>,
    Inject(db): Inject<Db>,
    Form(form): Form<CreateCommentForm>,
) -> Response {
    let claims = match actor.or_redirect() {
        Ok(claims) => claims,
        Err(response) => return response,
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
    /// The issue's displayed key (`WB-4`), not its id.
    pub target_key: String,
    pub kind: String,
}

#[post("/ui/issues/{id}/links")]
pub(super) async fn add_link(
    actor: ActorOrRedirect,
    Path(issue_id): Path<String>,
    Inject(db): Inject<Db>,
    Form(form): Form<AddLinkForm>,
) -> Response {
    let claims = match actor.or_redirect() {
        Ok(claims) => claims,
        Err(response) => return response,
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

/// `"WB-4"` -> `("WB", 4)`; splits from the right in case a key contains `-`.
fn split_issue_key(key: &str) -> Option<(String, i32)> {
    let (project_key, seq) = key.trim().rsplit_once('-')?;
    let seq: i32 = seq.trim().parse().ok()?;
    Some((project_key.trim().to_string(), seq))
}

#[post("/ui/issues/{id}/links/{link_id}/delete")]
pub(super) async fn remove_link(
    actor: ActorOrRedirect,
    Path((issue_id, link_id)): Path<(String, String)>,
    Inject(db): Inject<Db>,
) -> Response {
    let claims = match actor.or_redirect() {
        Ok(claims) => claims,
        Err(response) => return response,
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

fn redirect_issue(issue_id: &str, error: Option<&str>) -> Response {
    let base = ui_path(&format!("/issues/{issue_id}"));
    let location = match error {
        Some(code) => format!("{base}?error={code}"),
        None => base,
    };
    Response::new(StatusCode::FOUND).header("Location", location)
}

fn redirect_home(error: Option<&str>) -> Response {
    let location = error.map_or_else(
        || ui_path("/home"),
        |code| format!("{}?error={code}", ui_path("/home")),
    );
    Response::new(StatusCode::FOUND).header("Location", location)
}

pub(super) fn register_routes() {
    let _ = detail as fn(_, _, _, _) -> _;
    let _ = update as fn(_, _, _, _) -> _;
    let _ = create_comment as fn(_, _, _, _) -> _;
    let _ = add_link as fn(_, _, _, _) -> _;
    let _ = remove_link as fn(_, _, _) -> _;
}
