//! The screens, as functions from the state to a view.

use std::rc::Rc;

use quench_ui::form::Field;
use quench_ui::i18n::I18n;
use quench_ui::prelude::*;
use quench_ui::router::{Router, link};
use workbench_core::board::{columns, moved, short_time, status_key};
use workbench_core::draft::{CommentDraft, IssueDraft, LinkDraft, ProjectDraft};
use workbench_core::model::{KINDS, LINK_KINDS, PRIORITIES, STATUSES};
use workbench_core::{Issue, LinkedIssue, User};

use crate::live::{Problem, Request, State};

/// What every screen needs of the app.
#[derive(Clone)]
pub struct Env {
    pub t: Rc<I18n>,
    pub base: String,
    pub ask: Rc<dyn Fn(Request)>,
}

impl Env {
    fn path(&self, rest: &str) -> String {
        format!("{}/ui{rest}", self.base)
    }
}

fn waiting(env: &Env) -> View {
    p().attr("id", "waiting")
        .class("fui-empty")
        .text(env.t.t("ui_loading"))
        .into_view()
}

#[must_use]
pub fn not_found(env: &Env) -> View {
    div()
        .class("wb-page")
        .child(
            p().attr("id", "not-found")
                .class("fui-empty")
                .text(env.t.t("ui_not_found")),
        )
        .child(link(&env.path("/home")).text(env.t.t("ui_nav_projects")))
        .into_view()
}

/// The projects under "Projects", the open one marked: what goes under the brand in the side panel.
#[must_use]
pub(crate) fn sidebar(env: &Env, state: &State, router: &Router) -> View {
    let (env, projects, router) = (env.clone(), state.projects.clone(), router.clone());
    View::dynamic(move || {
        let here = router.location().path;
        let mut subs = div().class("fui-nav-sub");
        for project in projects.get().unwrap_or_default() {
            let href = env.path(&format!("/projects/{}/board", project.id));
            let mut item = link(&href)
                .class("fui-nav-subitem")
                .attr("data-project", project.key.clone())
                .text(project.title());
            if here == href {
                item = item.attr("aria-current", "page").class("wb-current");
            }
            subs = subs.child(item);
        }
        div()
            .child(
                link(&env.path("/home"))
                    .class("fui-nav-item")
                    .child(forge_shell::icon("columns"))
                    .child(span().text(env.t.t("ui_nav_projects"))),
            )
            .child(subs)
            .into_view()
    })
}

fn labelled(id: &str, text: &str, control: Element) -> Element {
    div()
        .class("wb-field")
        .child(label().attr("for", id.to_string()).text(text.to_string()))
        .child(control)
}

fn text_box(field: &Field) -> Element {
    field.input().class("wb-input")
}

fn area(field: &Field) -> Element {
    field.bind(textarea()).class("wb-input wb-area")
}

/// A `<select>` bound to `field`; `options` are `(value, text)`.
fn choice(field: &Field, options: Vec<(String, String)>) -> Element {
    let pick = field.clone();
    let rows = options.into_iter().map(|(value, text)| {
        let (field, v) = (field.clone(), value.clone());
        option()
            .attr("value", value)
            .attr_bool("selected", move || field.value() == v)
            .text(text)
            .into_view()
    });
    select()
        .class("wb-input")
        .attr("id", field.name().to_string())
        .attr("name", field.name().to_string())
        .on_input(move |v| pick.set(v))
        .children(rows)
}

fn plain(values: &[&str]) -> Vec<(String, String)> {
    values
        .iter()
        .map(|v| ((*v).to_string(), (*v).to_string()))
        .collect()
}

/// A line under a form that says why it was not sent.
fn form_error(error: &Signal<Option<&'static str>>) -> View {
    let error = error.clone();
    View::dynamic(move || {
        error.get().map_or(View::Empty, |why| {
            p().class("wb-error")
                .attr("role", "alert")
                .text(why)
                .into_view()
        })
    })
}

fn badge(text: &str, tone: &str) -> Element {
    span()
        .class(format!("wb-badge wb-{tone}"))
        .text(text.to_string())
}

fn priority_tone(priority: &str) -> &'static str {
    match priority {
        "high" => "red",
        "medium" => "amber",
        _ => "gray",
    }
}

fn status_tone(status: &str) -> &'static str {
    match status {
        "done" => "green",
        "blocked" | "rejected" => "red",
        "in-progress" => "blue",
        _ => "gray",
    }
}

// ── Home ─────────────────────────────────────────────────────────────────────

#[must_use]
pub(crate) fn home(env: &Env, state: &State) -> View {
    let (env, projects) = (env.clone(), state.projects.clone());
    let open = create_signal(false);
    let draft = ProjectDraft::default();
    let key = Field::new("project-key", &draft.key);
    let name = Field::new("project-name", &draft.name);
    let description = Field::new("project-description", &draft.description);
    let error: Signal<Option<&'static str>> = create_signal(None);

    let creating = {
        let (env, open, error) = (env.clone(), open.clone(), error.clone());
        let (key, name, description) = (key.clone(), name.clone(), description.clone());
        move || -> View {
            if !open.get() {
                return View::Empty;
            }
            let send = {
                let (env, error) = (env.clone(), error.clone());
                let (key, name, description) = (key.clone(), name.clone(), description.clone());
                move || {
                    let draft = ProjectDraft {
                        key: key.value_untracked(),
                        name: name.value_untracked(),
                        description: description.value_untracked(),
                    };
                    match draft.validate() {
                        Ok(body) => {
                            error.set(None);
                            (env.ask)(Request::CreateProject(body));
                        }
                        Err(why) => error.set(Some(why.message())),
                    }
                }
            };
            let cancel = {
                let open = open.clone();
                move || open.set(false)
            };
            form()
                .attr("id", "new-project")
                .class("wb-card wb-form")
                .on_submit(send)
                .child(labelled(
                    "project-key",
                    &env.t.t("ui_field_key"),
                    text_box(&key).attr("maxlength", "10"),
                ))
                .child(labelled(
                    "project-name",
                    &env.t.t("ui_field_name"),
                    text_box(&name),
                ))
                .child(labelled(
                    "project-description",
                    &env.t.t("ui_field_description"),
                    area(&description),
                ))
                .child(form_error(&error))
                .child(
                    div()
                        .class("wb-actions")
                        .child(
                            button()
                                .attr("type", "button")
                                .class("wb-btn")
                                .attr("id", "cancel-project")
                                .on_click(cancel)
                                .text(env.t.t("ui_cancel")),
                        )
                        .child(
                            button()
                                .attr("type", "submit")
                                .class("wb-btn wb-primary")
                                .attr("id", "create-project")
                                .text(env.t.t("ui_home_create_button")),
                        ),
                )
                .into_view()
        }
    };

    let list = {
        let env = env.clone();
        move || -> View {
            let Some(projects) = projects.get() else {
                return waiting(&env);
            };
            if projects.is_empty() {
                return p()
                    .attr("id", "no-projects")
                    .class("fui-empty")
                    .text(env.t.t("ui_home_no_projects"))
                    .into_view();
            }
            let cards = projects.iter().map(|project| {
                link(&env.path(&format!("/projects/{}/board", project.id)))
                    .class("wb-card wb-project")
                    .attr("data-project", project.key.clone())
                    .child(strong().text(project.title()))
                    .child(
                        project
                            .description
                            .clone()
                            .map(|d| p().class("wb-muted").text(d)),
                    )
                    .into_view()
            });
            div().class("wb-grid").children(cards).into_view()
        }
    };

    div()
        .class("wb-page")
        .child(
            div()
                .class("wb-head")
                .child(h1().text(env.t.t("ui_home_title")))
                .child(
                    button()
                        .attr("id", "new-project-button")
                        .class("wb-btn wb-primary")
                        .on_click(move || open.update(|o| *o = !*o))
                        .text(env.t.t("ui_home_new_project")),
                ),
        )
        .child(View::dynamic(creating))
        .child(View::dynamic(list))
        .into_view()
}

// ── Board ────────────────────────────────────────────────────────────────────

fn user_options(env: &Env, me: &str, users: &[User], current: &str) -> Vec<(String, String)> {
    let mut options = vec![
        (String::new(), env.t.t("ui_field_unassigned")),
        (me.to_string(), "Me".to_string()),
    ];
    for user in users.iter().filter(|u| u.username != me) {
        options.push((user.username.clone(), user.label().to_string()));
    }
    if !current.is_empty() && !options.iter().any(|(v, _)| v == current) {
        options.push((current.to_string(), current.to_string()));
    }
    options
}

fn card(env: &Env, state: &State, issue: &Issue) -> Element {
    let (id, ask, board) = (issue.id.clone(), Rc::clone(&env.ask), state.board.clone());
    let pick = select()
        .class("wb-input wb-small")
        .attr("id", format!("status-{}", issue.id))
        .attr("aria-label", env.t.t("ui_field_status"))
        .on_input(move |status| {
            // Shown at once; a refusal asks for the board again.
            let moving = board.get_untracked().and_then(|b| {
                moved(&b.issues, &id, status).map(|issues| (b.project.clone(), issues))
            });
            if let Some((project, issues)) = moving {
                board.set(Some(workbench_core::Board { project, issues }));
                ask(Request::Move {
                    issue: id.clone(),
                    status: status.to_string(),
                });
            }
        });
    let options = STATUSES.iter().map(|status| {
        let current = issue.status.clone();
        option()
            .attr("value", *status)
            .attr_bool("selected", move || current == *status)
            .text(env.t.t(&status_key(status)))
            .into_view()
    });
    div()
        .class("wb-card wb-ticket")
        .attr("data-issue", issue.id.clone())
        // Dragged to a column, the card does what its status select does (`forge-shell`'s script).
        .attr("draggable", "true")
        .attr("data-drag-id", issue.id.clone())
        .attr("data-drag-select", format!("status-{}", issue.id))
        .child(div().class("wb-key").text(format!("#{}", issue.seq)))
        .child(
            link(&env.path(&format!("/issues/{}", issue.id)))
                .class("wb-ticket-title")
                // A link drags by itself and would win over the card's own drag.
                .attr("draggable", "false")
                .text(issue.title.clone()),
        )
        .child(
            div()
                .class("wb-meta")
                .child(badge(&issue.kind, "gray"))
                .child(badge(&issue.priority, priority_tone(&issue.priority)))
                .child(issue.estimate.map(|e| span().text(format!("{e} pt")))),
        )
        .child(pick.children(options))
}

#[must_use]
pub(crate) fn board(env: &Env, state: &State, id: String) -> View {
    let (env, state) = (env.clone(), state.clone());
    let open = create_signal(false);
    let draft = IssueDraft::default();
    let title = Field::new("issue-title", &draft.title);
    let kind = Field::new("issue-kind", &draft.kind);
    let priority = Field::new("issue-priority", &draft.priority);
    let assignee = Field::new("issue-assignee", &draft.assignee);
    let error: Signal<Option<&'static str>> = create_signal(None);

    let creating = {
        let (env, state, open, error) = (env.clone(), state.clone(), open.clone(), error.clone());
        let (title, kind, priority, assignee) = (
            title.clone(),
            kind.clone(),
            priority.clone(),
            assignee.clone(),
        );
        let project = id.clone();
        move || -> View {
            if !open.get() {
                return View::Empty;
            }
            let users = state.users.get();
            let me = state.me.get();
            let send = {
                let (env, error, open, project) =
                    (env.clone(), error.clone(), open.clone(), project.clone());
                let (title, kind, priority, assignee) = (
                    title.clone(),
                    kind.clone(),
                    priority.clone(),
                    assignee.clone(),
                );
                move || {
                    let draft = IssueDraft {
                        title: title.value_untracked(),
                        kind: kind.value_untracked(),
                        priority: priority.value_untracked(),
                        assignee: assignee.value_untracked(),
                        ..IssueDraft::default()
                    };
                    match draft.to_create() {
                        Ok(body) => {
                            error.set(None);
                            title.set("");
                            open.set(false);
                            (env.ask)(Request::CreateIssue {
                                project: project.clone(),
                                body,
                            });
                        }
                        Err(why) => error.set(Some(why.message())),
                    }
                }
            };
            let cancel = {
                let open = open.clone();
                move || open.set(false)
            };
            form()
                .attr("id", "new-issue")
                .class("wb-card wb-form")
                .on_submit(send)
                .child(labelled(
                    "issue-title",
                    &env.t.t("ui_field_title"),
                    text_box(&title),
                ))
                .child(
                    div()
                        .class("wb-row")
                        .child(labelled(
                            "issue-kind",
                            &env.t.t("ui_field_kind"),
                            choice(&kind, plain(&KINDS)),
                        ))
                        .child(labelled(
                            "issue-priority",
                            &env.t.t("ui_field_priority"),
                            choice(&priority, plain(&PRIORITIES)),
                        ))
                        .child(labelled(
                            "issue-assignee",
                            &env.t.t("ui_field_assignee"),
                            choice(&assignee, user_options(&env, &me, &users, "")),
                        )),
                )
                .child(form_error(&error))
                .child(
                    div()
                        .class("wb-actions")
                        .child(
                            button()
                                .attr("type", "button")
                                .class("wb-btn")
                                .on_click(cancel)
                                .text(env.t.t("ui_cancel")),
                        )
                        .child(
                            button()
                                .attr("type", "submit")
                                .class("wb-btn wb-primary")
                                .attr("id", "create-issue")
                                .text(env.t.t("ui_board_create_button")),
                        ),
                )
                .into_view()
        }
    };

    let columns_view = {
        let (env, state) = (env.clone(), state.clone());
        move || -> View {
            if state.problem.get() == Some(Problem::NotFound) {
                return not_found(&env);
            }
            let Some(data) = state.board.get().filter(|b| b.project.id == id) else {
                return waiting(&env);
            };
            let head = div()
                .class("wb-head")
                .child(h1().attr("id", "project-title").text(data.project.title()))
                .child(
                    button()
                        .attr("id", "new-issue-button")
                        .class("wb-btn wb-primary")
                        .on_click({
                            let open = open.clone();
                            move || open.update(|o| *o = !*o)
                        })
                        .text(env.t.t("ui_board_new_issue")),
                );
            let cols = columns(&data.issues).into_iter().map(|(status, issues)| {
                let cards = issues
                    .into_iter()
                    .map(|i| card(&env, &state, i).into_view());
                div()
                    .class("wb-column")
                    .attr("data-status", status)
                    .attr("data-drop-value", status)
                    .child(
                        div()
                            .class("wb-column-title")
                            .text(env.t.t(&status_key(status))),
                    )
                    .child(div().class("wb-column-body").children(cards))
                    .into_view()
            });
            div()
                .class("wb-page wb-board-page")
                .child(head)
                .child(View::dynamic(creating.clone()))
                .child(div().class("wb-board").children(cols))
                .into_view()
        }
    };
    View::dynamic(columns_view)
}

// ── Issue ────────────────────────────────────────────────────────────────────

fn link_row(env: &Env, issue: &str, row: &LinkedIssue) -> Element {
    let (ask, issue_id, link_id) = (Rc::clone(&env.ask), issue.to_string(), row.link_id.clone());
    div()
        .class("wb-link-row")
        .attr("data-link", row.link_id.clone())
        .child(
            link(&env.path(&format!("/issues/{}", row.issue_id)))
                .class("wb-link-title")
                .text(format!("{}-{} — {}", row.project_key, row.seq, row.title)),
        )
        .child(badge(&row.status, status_tone(&row.status)))
        .child(
            button()
                .attr("type", "button")
                .class("wb-btn wb-small wb-remove")
                .attr("title", env.t.t("ui_remove"))
                .attr("aria-label", env.t.t("ui_remove"))
                .on_click(move || {
                    ask(Request::RemoveLink {
                        issue: issue_id.clone(),
                        link: link_id.clone(),
                    });
                })
                .text("×"),
        )
}

fn link_section(env: &Env, issue: &str, key: &str, rows: &[LinkedIssue]) -> Element {
    let mut list = div().class("wb-link-list");
    if rows.is_empty() {
        list = list.child(p().class("wb-muted").text(env.t.t("ui_issue_no_links")));
    }
    for row in rows {
        list = list.child(link_row(env, issue, row));
    }
    div()
        .class("wb-link-section")
        .attr("data-section", key.to_string())
        .child(div().class("wb-subtitle").text(env.t.t(key)))
        .child(list)
}

#[allow(clippy::too_many_lines)]
fn issue_page(env: &Env, state: &State, view: &workbench_core::IssueView) -> View {
    let issue = view.issue.clone();
    let me = state.me.get();
    let users = state.users.get();
    let draft = IssueDraft::from_issue(&issue);
    let title = Field::new("title", &draft.title);
    let description = Field::new("description", &draft.description);
    let status = Field::new("status", &draft.status);
    let kind = Field::new("kind", &draft.kind);
    let priority = Field::new("priority", &draft.priority);
    let assignee = Field::new("assignee", &draft.assignee);
    let estimate = Field::new("estimate", &draft.estimate);
    let error: Signal<Option<&'static str>> = create_signal(None);

    let save = {
        let (env, error, issue_id, current) = (
            env.clone(),
            error.clone(),
            issue.id.clone(),
            issue.status.clone(),
        );
        let fields = (
            title.clone(),
            description.clone(),
            status.clone(),
            kind.clone(),
            priority.clone(),
            assignee.clone(),
            estimate.clone(),
        );
        move || {
            let draft = IssueDraft {
                title: fields.0.value_untracked(),
                description: fields.1.value_untracked(),
                status: fields.2.value_untracked(),
                kind: fields.3.value_untracked(),
                priority: fields.4.value_untracked(),
                assignee: fields.5.value_untracked(),
                estimate: fields.6.value_untracked(),
            };
            match draft.to_update() {
                Ok(body) => {
                    error.set(None);
                    (env.ask)(Request::Save {
                        issue: issue_id.clone(),
                        body,
                        status: (draft.status != current).then_some(draft.status),
                    });
                }
                Err(why) => error.set(Some(why.message())),
            }
        }
    };
    let assign_me = {
        let (assignee, me) = (assignee.clone(), me.clone());
        move || assignee.set(&me)
    };

    let details = form()
        .attr("id", "details")
        .class("wb-card wb-form")
        .on_submit(save)
        .child(
            div()
                .class("wb-head")
                .child(h3().text(env.t.t("ui_issue_details")))
                .child(badge(&issue.key(&view.project.key), "blue")),
        )
        .child(labelled(
            "title",
            &env.t.t("ui_field_title"),
            text_box(&title).attr("required", "true"),
        ))
        .child(labelled(
            "description",
            &env.t.t("ui_field_description"),
            area(&description),
        ))
        .child(
            div()
                .class("wb-row")
                .child(labelled(
                    "status",
                    &env.t.t("ui_field_status"),
                    choice(&status, plain(&STATUSES)),
                ))
                .child(labelled(
                    "kind",
                    &env.t.t("ui_field_kind"),
                    choice(&kind, plain(&KINDS)),
                ))
                .child(labelled(
                    "priority",
                    &env.t.t("ui_field_priority"),
                    choice(&priority, plain(&PRIORITIES)),
                ))
                .child(labelled(
                    "assignee",
                    &env.t.t("ui_field_assignee"),
                    div()
                        .class("wb-inline")
                        .child(choice(
                            &assignee,
                            user_options(env, &me, &users, &draft.assignee),
                        ))
                        .child(
                            button()
                                .attr("type", "button")
                                .attr("id", "assign-me")
                                .class("wb-btn wb-small")
                                .on_click(assign_me)
                                .text(env.t.t("ui_assign_to_me")),
                        ),
                ))
                .child(labelled(
                    "estimate",
                    &env.t.t("ui_field_estimate"),
                    text_box(&estimate).attr("inputmode", "numeric"),
                )),
        )
        .child(form_error(&error))
        .child(
            div().class("wb-actions").child(
                button()
                    .attr("type", "submit")
                    .attr("id", "save-issue")
                    .class("wb-btn wb-primary")
                    .text(env.t.t("ui_issue_save_button")),
            ),
        );

    let link_draft = LinkDraft::default();
    let target = Field::new("link-target", &link_draft.key);
    let link_kind = Field::new("link-kind", &link_draft.kind);
    let link_error: Signal<Option<&'static str>> = create_signal(None);
    let add_link = {
        let (env, link_error, issue_id) = (env.clone(), link_error.clone(), issue.id.clone());
        let (target, link_kind) = (target.clone(), link_kind.clone());
        move || {
            let draft = LinkDraft {
                key: target.value_untracked(),
                kind: link_kind.value_untracked(),
            };
            match draft.target() {
                Ok(key) => {
                    link_error.set(None);
                    (env.ask)(Request::AddLink {
                        issue: issue_id.clone(),
                        key,
                        kind: draft.kind,
                    });
                }
                Err(why) => link_error.set(Some(why.message())),
            }
        }
    };
    let dependencies = div()
        .class("wb-card")
        .attr("id", "dependencies")
        .child(h3().text(env.t.t("ui_issue_dependencies")))
        .child(link_section(
            env,
            &issue.id,
            "ui_issue_blocks",
            &view.links.blocks,
        ))
        .child(link_section(
            env,
            &issue.id,
            "ui_issue_blocked_by",
            &view.links.blocked_by,
        ))
        .child(link_section(
            env,
            &issue.id,
            "ui_issue_relates_to",
            &view.links.relates_to,
        ))
        .child(
            form()
                .attr("id", "add-link")
                .class("wb-form")
                .on_submit(add_link)
                .child(
                    div()
                        .class("wb-row")
                        .child(labelled(
                            "link-target",
                            &env.t.t("ui_field_link_target"),
                            text_box(&target).attr("placeholder", "e.g. WB-4"),
                        ))
                        .child(labelled(
                            "link-kind",
                            &env.t.t("ui_field_link_kind"),
                            choice(
                                &link_kind,
                                LINK_KINDS
                                    .iter()
                                    .map(|k| ((*k).to_string(), k.replace('_', " ")))
                                    .collect(),
                            ),
                        )),
                )
                .child(form_error(&link_error))
                .child(
                    div().class("wb-actions").child(
                        button()
                            .attr("type", "submit")
                            .class("wb-btn")
                            .attr("id", "add-link-button")
                            .text(env.t.t("ui_issue_add_link_button")),
                    ),
                ),
        );

    let body = Field::new("comment-body", "");
    let comment_error: Signal<Option<&'static str>> = create_signal(None);
    let add_comment = {
        let (env, comment_error, issue_id, body) = (
            env.clone(),
            comment_error.clone(),
            issue.id.clone(),
            body.clone(),
        );
        move || match (CommentDraft {
            body: body.value_untracked(),
        })
        .validate()
        {
            Ok(comment) => {
                comment_error.set(None);
                (env.ask)(Request::Comment {
                    issue: issue_id.clone(),
                    body: comment.body,
                });
            }
            Err(why) => comment_error.set(Some(why.message())),
        }
    };
    let mut thread = div().class("wb-thread");
    if view.comments.is_empty() {
        thread = thread.child(
            p().attr("id", "no-comments")
                .class("wb-muted")
                .text(env.t.t("ui_issue_empty_comments")),
        );
    }
    for comment in &view.comments {
        thread = thread.child(
            div()
                .class("wb-comment")
                .child(
                    div()
                        .class("wb-comment-head")
                        .child(strong().text(comment.author.clone()))
                        .child(
                            span()
                                .class("wb-muted")
                                .text(short_time(&comment.created_at)),
                        ),
                )
                .child(p().class("wb-comment-body").text(comment.body.clone())),
        );
    }
    let comments = div()
        .class("wb-card")
        .attr("id", "comments")
        .child(h3().text(env.t.t("ui_issue_comments")))
        .child(thread)
        .child(
            form()
                .attr("id", "add-comment")
                .class("wb-form")
                .on_submit(add_comment)
                .child(labelled(
                    "comment-body",
                    &env.t.t("ui_field_comment_body"),
                    area(&body),
                ))
                .child(form_error(&comment_error))
                .child(
                    div().class("wb-actions").child(
                        button()
                            .attr("type", "submit")
                            .attr("id", "add-comment-button")
                            .class("wb-btn wb-primary")
                            .text(env.t.t("ui_issue_add_comment_button")),
                    ),
                ),
        );

    div()
        .class("wb-page")
        .child(
            div()
                .class("wb-crumbs")
                .child(link(&env.path("/home")).text(env.t.t("ui_nav_projects")))
                .child(span().text(" / "))
                .child(
                    link(&env.path(&format!("/projects/{}/board", view.project.id)))
                        .attr("id", "crumb-project")
                        .text(view.project.title()),
                )
                .child(span().text(" / "))
                .child(span().text(issue.key(&view.project.key))),
        )
        .child(h1().attr("id", "issue-title").text(issue.title.clone()))
        .child(details)
        .child(dependencies)
        .child(comments)
        .into_view()
}

#[must_use]
pub(crate) fn issue(env: &Env, state: &State, id: String) -> View {
    let (env, state) = (env.clone(), state.clone());
    View::dynamic(move || {
        if state.problem.get() == Some(Problem::NotFound) {
            return not_found(&env);
        }
        match state.issue.get() {
            Some(view) if view.issue.id == id => issue_page(&env, &state, &view),
            _ => waiting(&env),
        }
    })
}
