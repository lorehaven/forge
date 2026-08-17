//! A project's board: one column per fixed-workflow status.

use crate::domain::issue::{self, NewIssue, STATUSES};
use crate::domain::{project, realm_users};
use crate::routers::api::authz::can_on_project_claims;
use crate::routers::ui::common::{
    Notice, actor, assignee_field, is_ui_authenticated, notice_banner, render_page,
    ui_login_redirect, ui_login_redirect_for, ui_path,
};
use actix_web::{HttpRequest, HttpResponse, Responder, get, post, web};
use quench_auth::prelude::JwtConfig;
use quench_db::prelude::Db;
use quench_web::framework::dom::toggle_modal;
use quench_web::prelude::*;
use serde::Deserialize;

#[get("/projects/{id}/board")]
pub(super) async fn board(
    req: HttpRequest,
    project_id: web::Path<String>,
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

    let Some(project) = project::read(&db, &project_id).await.ok().flatten() else {
        return HttpResponse::Found()
            .append_header(("Location", ui_path("/home")))
            .finish();
    };

    let board_el = render_board(&db, &project.id).await;
    let users = realm_users::list_users(&db).await.unwrap_or_default();
    let toggle = toggle_modal("modal-overlay", "modal-center", "show");

    render_page(
        HttpResponse::Ok(),
        content().class("home-content").child(
            div()
                .class("home-container")
                .child_opt(notice_banner(&notice))
                .child(
                    div()
                        .class("home-header")
                        .child(h3().text(format!("{} — {}", project.key, project.name)))
                        .child(
                            button()
                                .attr("type", "button")
                                .attr("onclick", &toggle)
                                .attr("title", "New issue")
                                .class("wb-add-button")
                                .text("+"),
                        ),
                )
                .child(board_el)
                .child(div().class("modal-overlay").on_click(&toggle))
                .child(div().class("modal-center").child(new_issue_modal_content(
                    &project.id,
                    &toggle,
                    &claims.sub,
                    &users,
                )))
                .child(board_script()),
        ),
    )
}

/// One column per status, rebuilt fresh - shared by the full page (`board`)
/// and the htmx fragment `transition_issue` returns after a status change (be
/// it from the fallback `<select>` or a drag-and-drop, see `board_script`).
async fn render_board(db: &Db, project_id: &str) -> Element {
    let mut row = div()
        .class("wb-board")
        .attr("id", "wb-board")
        .attr("data-project-id", project_id);

    for status in STATUSES {
        let issues = issue::list_by_project(db, project_id, Some(status))
            .await
            .unwrap_or_default();

        let mut body = div().class("wb-column-body").attr("data-status", status);
        for i in &issues {
            body = body.child(issue_card(project_id, i));
        }

        row = row.child(
            div()
                .class("wb-column")
                .child(
                    div()
                        .class("wb-column-title")
                        .attr("data-i18n", status_i18n_key(status)),
                )
                .child(body),
        );
    }

    row
}

fn issue_card(project_id: &str, i: &issue::Issue) -> Element {
    let mut status_select = select()
        .attr("name", "status")
        .attr(
            "hx-post",
            ui_path(&format!(
                "/projects/{project_id}/issues/{}/transition",
                i.id
            )),
        )
        .attr("hx-target", "#wb-board")
        .attr("hx-swap", "outerHTML")
        .attr("hx-trigger", "change");

    for status in STATUSES {
        let mut opt = option().attr("value", status).text(status);
        if status == i.status {
            opt = opt.attr("selected", "true");
        }
        status_select = status_select.child(opt);
    }

    div()
        .class("wb-card")
        .attr("draggable", "true")
        .attr("data-issue-id", &i.id)
        .child(div().class("wb-card-key").text(format!("#{}", i.seq)))
        .child(
            a().attr("href", ui_path(&format!("/issues/{}", i.id)))
                // Links are natively draggable in every browser, and being
                // the nearest draggable element under the pointer wins over
                // an ancestor's `draggable="true"` - grabbing a card by its
                // title (the natural place to grab it) would otherwise start
                // a browser link-drag instead of `board_script`'s card drag.
                .attr("draggable", "false")
                .class("wb-card-title")
                .text(i.title.clone()),
        )
        .child(
            div()
                .class("wb-card-meta")
                .child(span().text(i.kind.clone()))
                .child(span().text(i.priority.clone()))
                .child_opt(
                    i.estimate
                        .map(|estimate| span().text(format!("{estimate} pt"))),
                ),
        )
        .child(status_select)
}

/// Drag a card onto a column to transition it, as an alternative to the
/// per-card `<select>` (`issue_card`) - which stays as the fallback and is
/// what this reuses for the actual request, so the two controls can never
/// disagree about the URL a status change posts to.
///
/// Delegated on `document` rather than bound to `#wb-board`'s own children:
/// `transition_issue`'s htmx response replaces `#wb-board` wholesale (see
/// `render_board`'s doc comment), which would tear down any listener attached
/// directly to a card or column. The `__wbDndInit` guard matters because this
/// script is only ever emitted once, on the full page load - the htmx
/// fragment is `render_board`'s output alone - but is cheap insurance against
/// a future caller including it twice.
fn board_script() -> Element {
    script(
        r##"
(function () {
    if (window.__wbDndInit) return;
    window.__wbDndInit = true;

    document.addEventListener("dragstart", function (event) {
        var card = event.target.closest(".wb-card");
        if (!card) return;
        event.dataTransfer.setData("text/plain", card.dataset.issueId);
        event.dataTransfer.effectAllowed = "move";
    });

    document.addEventListener("dragover", function (event) {
        if (!event.target.closest(".wb-column-body")) return;
        event.preventDefault();
        event.dataTransfer.dropEffect = "move";
    });

    document.addEventListener("dragenter", function (event) {
        var body = event.target.closest(".wb-column-body");
        if (body) body.classList.add("wb-drop-target");
    });

    document.addEventListener("dragleave", function (event) {
        var body = event.target.closest(".wb-column-body");
        if (body && !body.contains(event.relatedTarget)) {
            body.classList.remove("wb-drop-target");
        }
    });

    document.addEventListener("drop", function (event) {
        var body = event.target.closest(".wb-column-body");
        if (!body) return;
        event.preventDefault();
        body.classList.remove("wb-drop-target");

        var issueId = event.dataTransfer.getData("text/plain");
        if (!issueId) return;
        var card = document.querySelector('[data-issue-id="' + issueId + '"]');
        if (!card) return;
        var select = card.querySelector("select[name=status]");
        if (!select) return;
        var status = body.dataset.status;
        if (select.value === status) return;

        htmx.ajax("POST", select.getAttribute("hx-post"), {
            target: "#wb-board",
            swap: "outerHTML",
            values: { status: status }
        });
    });
})();
"##
        .to_string(),
    )
    .raw()
}

fn status_i18n_key(status: &str) -> &'static str {
    match status {
        "blocked" => "ui_status_blocked",
        "todo" => "ui_status_todo",
        "in-progress" => "ui_status_in_progress",
        "done" => "ui_status_done",
        "rejected" => "ui_status_rejected",
        _ => "ui_status_todo",
    }
}

fn new_issue_modal_content(
    project_id: &str,
    toggle: &str,
    current_user: &str,
    users: &[realm_users::RealmUser],
) -> Element {
    div()
        .class("modal-content")
        .child(
            div()
                .class("wb-modal-header")
                .child(
                    div()
                        .class("panel-title")
                        .attr("data-i18n", "ui_board_new_issue"),
                )
                .child(
                    button()
                        .attr("type", "button")
                        .attr("onclick", toggle)
                        .class("wb-modal-close")
                        .text("×"),
                ),
        )
        .child(
            form()
                .attr("method", "post")
                .attr("action", ui_path(&format!("/projects/{project_id}/issues")))
                .class("wb-form")
                .child(
                    div()
                        .class("wb-field-row")
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
                                .attr("required", "true"),
                        ),
                )
                .child(
                    div()
                        .class("wb-field-row")
                        .child(
                            label()
                                .attr("for", "wb-kind")
                                .attr("data-i18n", "ui_field_kind"),
                        )
                        .child(
                            select()
                                .attr("id", "wb-kind")
                                .attr("name", "kind")
                                .child(option().attr("value", "task").text("task"))
                                .child(option().attr("value", "bug").text("bug"))
                                .child(option().attr("value", "story").text("story")),
                        ),
                )
                .child(
                    div()
                        .class("wb-field-row")
                        .child(
                            label()
                                .attr("for", "wb-priority")
                                .attr("data-i18n", "ui_field_priority"),
                        )
                        .child(
                            select()
                                .attr("id", "wb-priority")
                                .attr("name", "priority")
                                .child(option().attr("value", "low").text("low"))
                                .child(
                                    option()
                                        .attr("value", "medium")
                                        .attr("selected", "true")
                                        .text("medium"),
                                )
                                .child(option().attr("value", "high").text("high")),
                        ),
                )
                .child(
                    div()
                        .class("wb-field-row")
                        .child(
                            label()
                                .attr("for", "wb-assignee")
                                .attr("data-i18n", "ui_field_assignee"),
                        )
                        .child(assignee_field(current_user, users, None)),
                )
                .child(
                    button()
                        .attr("type", "submit")
                        .class("wb-submit")
                        .attr("data-i18n", "ui_board_create_button"),
                ),
        )
}

#[derive(Deserialize)]
pub(super) struct CreateIssueForm {
    pub title: String,
    #[serde(default = "default_kind")]
    pub kind: String,
    #[serde(default = "default_priority")]
    pub priority: String,
    #[serde(default)]
    pub assignee: String,
}

fn default_kind() -> String {
    "task".to_string()
}

fn default_priority() -> String {
    "medium".to_string()
}

#[post("/projects/{id}/issues")]
pub(super) async fn create_issue(
    request: HttpRequest,
    project_id: web::Path<String>,
    config: web::Data<JwtConfig>,
    db: web::Data<Db>,
    form: web::Form<CreateIssueForm>,
) -> impl Responder {
    let Some(claims) = actor(&request, &config).await else {
        return ui_login_redirect_for(&request);
    };

    if form.title.trim().is_empty() {
        return redirect_board(&project_id, Some("title_required"));
    }

    if !can_on_project_claims(&claims, &project_id, "write") {
        return redirect_board(&project_id, Some("forbidden"));
    }

    let new = NewIssue {
        project_id: project_id.to_string(),
        parent_id: None,
        kind: form.kind.clone(),
        title: form.title.trim().to_string(),
        description: None,
        priority: form.priority.clone(),
        assignee: (!form.assignee.trim().is_empty()).then(|| form.assignee.trim().to_string()),
        reporter: claims.sub.clone(),
        estimate: None,
    };

    match issue::create(&db, &new).await {
        Ok(_) => redirect_board(&project_id, None),
        // The only user-typed field here that's a foreign key is `assignee`
        // (`workbench.issues.assignee references auth.users(username)`), so
        // a violation almost certainly means that username doesn't exist.
        Err(error) if error.is_foreign_key_violation() => {
            redirect_board(&project_id, Some("unknown_assignee"))
        }
        Err(_) => redirect_board(&project_id, Some("create_failed")),
    }
}

#[derive(Deserialize)]
pub(super) struct TransitionForm {
    pub status: String,
}

/// The board's own drag-free "move a card" control: a status `<select>` per
/// card that posts here via htmx and swaps in the freshly rendered board -
/// see `issue_card`.
#[post("/projects/{project_id}/issues/{issue_id}/transition")]
pub(super) async fn transition_issue(
    request: HttpRequest,
    path: web::Path<(String, String)>,
    config: web::Data<JwtConfig>,
    db: web::Data<Db>,
    form: web::Form<TransitionForm>,
) -> impl Responder {
    let (project_id, issue_id) = path.into_inner();

    let Some(claims) = actor(&request, &config).await else {
        return ui_login_redirect_for(&request);
    };

    if issue::is_valid_status(&form.status) && can_on_project_claims(&claims, &project_id, "write")
    {
        let _ = issue::transition(&db, &issue_id, &form.status).await;
    }

    let board_el = render_board(&db, &project_id).await;
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(board_el.render())
}

fn redirect_board(project_id: &str, error: Option<&str>) -> HttpResponse {
    let base = ui_path(&format!("/projects/{project_id}/board"));
    let location = match error {
        Some(code) => format!("{base}?error={code}"),
        None => base,
    };
    HttpResponse::Found()
        .append_header(("Location", location))
        .finish()
}
