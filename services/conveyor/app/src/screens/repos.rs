//! Registering, editing, removing and following repositories, and the "run with parameters" form.

use std::rc::Rc;

use conveyor_core::tree::project_path;
use conveyor_core::{LaunchDraft, Project, Repo, RepoDraft};
use quench_ui::form::Field;
use quench_ui::prelude::*;
use quench_ui::router::link;

use super::{Env, back, enabled_badge, labelled, nothing, panel, project_label, waiting};
use crate::live::{Follow, Notice, Request, State};

/// The repository `owner/name`, once the repositories are there: `None` while they load, `Some(None)` if there is none.
fn find(state: &State, owner: &str, name: &str) -> Option<Option<Repo>> {
    state.repos.with(|repos| {
        repos.as_ref().map(|repos| {
            repos
                .iter()
                .find(|r| r.owner == owner && r.name == name)
                .cloned()
        })
    })
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
        .class("cv-input")
        .attr("id", field.name().to_string())
        .attr("name", field.name().to_string())
        .on_input(move |v| pick.set(v))
        .children(rows)
}

fn text_box(field: &Field) -> Element {
    field.input().class("cv-input")
}

fn project_options(all: &[Project], ids: &[String]) -> Vec<(String, String)> {
    all.iter()
        .filter(|p| ids.contains(&p.id))
        .map(|p| (p.id.clone(), project_path(&p.id, all)))
        .collect()
}

// ── The list ─────────────────────────────────────────────────────────────────

pub(crate) fn repos(env: &Env, state: &State) -> View {
    let (env, state) = (env.clone(), state.clone());
    div()
        .attr("id", "repos")
        .class("cv-page cv-narrow")
        .child(back(&env, &env.path("/home"), "ui_repos_back_home"))
        .child(list_panel(&env, &state))
        .child(follow_panel(&env, &state))
        .child(create_panel(&env, &state))
        .into_view()
}

fn follow_button(env: &Env, target: Follow, following: bool, id: &str) -> Element {
    let (ask, goes) = (Rc::clone(&env.ask), target);
    button()
        .attr("type", "button")
        .attr("id", format!("follow-{id}"))
        .class(if following {
            "cv-btn cv-follow cv-following"
        } else {
            "cv-btn cv-follow"
        })
        .text(env.t.t(if following {
            "ui_follow_stop"
        } else {
            "ui_follow_start"
        }))
        .on_click(move || {
            ask(Request::Follow {
                target: goes.clone(),
                on: !following,
            });
        })
}

fn list_panel(env: &Env, state: &State) -> View {
    let (env, state) = (env.clone(), state.clone());
    View::dynamic(move || {
        let body = state.repos.with(|repos| {
            let Some(repos) = repos.as_ref() else {
                return waiting(&env);
            };
            if repos.is_empty() {
                return nothing(&env, "ui_repos_empty", "no-repos").into_view();
            }
            let projects = state.projects.get().unwrap_or_default();
            let access = state.access.get();
            let subs = state.subs.get();
            let mut rows = div().class("cv-meta-list");
            for repo in repos {
                let can_edit = access
                    .as_ref()
                    .is_some_and(|a| a.can_write(&repo.project_id));
                let following = subs.iter().any(|s| s.repo_id.as_deref() == Some(&repo.id));
                rows = rows.child(
                    div()
                        .class("cv-row")
                        .attr("data-repo", repo.id.clone())
                        .child(
                            div()
                                .class("cv-row-main")
                                .child(span().class("cv-repo-name").text(repo.slug()))
                                .child(
                                    span()
                                        .class("cv-project-path")
                                        .text(project_path(&repo.project_id, &projects)),
                                ),
                        )
                        .child(
                            div()
                                .class("cv-row-meta")
                                .child(span().class("cv-muted").text(repo.provider.to_string()))
                                .child(span().class("cv-mono").text(repo.default_branch.clone()))
                                .child(enabled_badge(&env, repo.enabled)),
                        )
                        .child(can_edit.then(|| {
                            link(&env.repo_path(repo, "/edit"))
                                .attr("id", format!("edit-{}", repo.id))
                                .class("cv-btn cv-edit")
                                .text(env.t.t("ui_repos_edit"))
                        }))
                        .child(follow_button(
                            &env,
                            Follow::Repo(repo.id.clone()),
                            following,
                            &repo.id,
                        )),
                );
            }
            rows.into_view()
        });
        panel(div().text(env.t.t("ui_repos_title")))
            .child(body)
            .into_view()
    })
}

/// Every project the caller can read, each with a follow toggle: following one covers everything nested beneath it.
fn follow_panel(env: &Env, state: &State) -> View {
    let (env, state) = (env.clone(), state.clone());
    View::dynamic(move || {
        let projects = state.projects.get().unwrap_or_default();
        let Some(access) = state.access.get() else {
            return View::Empty;
        };
        let readable: Vec<&Project> = projects.iter().filter(|p| access.can_read(&p.id)).collect();
        if readable.is_empty() {
            return View::Empty;
        }
        let subs = state.subs.get();
        let mut rows = div().class("cv-meta-list");
        for project in readable {
            let following = subs
                .iter()
                .any(|s| s.project_id.as_deref() == Some(&project.id));
            rows = rows.child(
                div()
                    .class("cv-row")
                    .attr("data-project", project.id.clone())
                    .child(
                        div().class("cv-row-main").child(
                            span()
                                .class("cv-project-path")
                                .text(project_path(&project.id, &projects)),
                        ),
                    )
                    .child(follow_button(
                        &env,
                        Follow::Project(project.id.clone()),
                        following,
                        &project.id,
                    )),
            );
        }
        panel(div().text(env.t.t("ui_follow_projects_title")))
            .attr("id", "follow-projects")
            .child(
                p().class("cv-hint")
                    .text(env.t.t("ui_follow_projects_hint")),
            )
            .child(rows)
            .into_view()
    })
}

/// Omitted, not disabled, when there is nowhere the caller may register a repository.
fn create_panel(env: &Env, state: &State) -> View {
    let (env, state) = (env.clone(), state.clone());
    let draft = RepoDraft::default();
    let owner = Field::new("new-owner", &draft.owner);
    let name = Field::new("new-name", &draft.name);
    let clone_url = Field::new("new-clone-url", &draft.clone_url);
    let branch = Field::new("new-branch", &draft.default_branch);
    let project = Field::new("new-project", &draft.project_id);
    let provider = Field::new("new-provider", &draft.provider);

    // A repository that was registered leaves an empty form.
    {
        let notice = state.notice.clone();
        let fields = [
            owner.clone(),
            name.clone(),
            clone_url.clone(),
            branch.clone(),
        ];
        create_effect(move || {
            if matches!(notice.get(), Some(Notice::Ok(key)) if key == "ui_repos_ok_created") {
                untrack(|| {
                    for field in &fields {
                        field.reset("");
                    }
                });
            }
        });
    }

    View::dynamic(move || {
        let projects = state.projects.get().unwrap_or_default();
        let Some(access) = state.access.get() else {
            return View::Empty;
        };
        let options = project_options(&projects, &access.write);
        let Some(first) = options.first().map(|(id, _)| id.clone()) else {
            return View::Empty;
        };
        if project.value_untracked().is_empty() {
            project.set(&first);
        }
        let send = {
            let (ask, notice) = (Rc::clone(&env.ask), state.notice.clone());
            let (owner, name, clone_url, branch, project, provider) = (
                owner.clone(),
                name.clone(),
                clone_url.clone(),
                branch.clone(),
                project.clone(),
                provider.clone(),
            );
            move || {
                let draft = RepoDraft {
                    owner: owner.value_untracked(),
                    name: name.value_untracked(),
                    clone_url: clone_url.value_untracked(),
                    default_branch: branch.value_untracked(),
                    project_id: project.value_untracked(),
                    provider: provider.value_untracked(),
                    enabled: true,
                };
                match draft.to_register() {
                    Ok(body) => {
                        notice.set(None);
                        ask(Request::CreateRepo(body));
                    }
                    Err(why) => notice.set(Some(Notice::ErrKey(why.key().to_string()))),
                }
            }
        };
        let form = form()
            .attr("id", "new-repo")
            .class("cv-form")
            .on_submit(send)
            .child(labelled(
                "new-owner",
                &env.t.t("ui_repos_owner"),
                text_box(&owner),
            ))
            .child(labelled(
                "new-name",
                &env.t.t("ui_repos_name"),
                text_box(&name),
            ))
            .child(labelled(
                "new-clone-url",
                &env.t.t("ui_repos_clone_url"),
                text_box(&clone_url),
            ))
            .child(labelled(
                "new-branch",
                &env.t.t("ui_repos_branch"),
                text_box(&branch).attr("placeholder", "master"),
            ))
            .child(labelled(
                "new-project",
                &env.t.t("ui_repos_project"),
                choice(&project, options),
            ))
            .child(labelled(
                "new-provider",
                &env.t.t("ui_repos_provider"),
                choice(
                    &provider,
                    vec![
                        ("github".to_string(), "GitHub".to_string()),
                        ("generic".to_string(), "Generic".to_string()),
                    ],
                ),
            ))
            .child(
                div().class("cv-actions-row").child(
                    button()
                        .attr("type", "submit")
                        .attr("id", "create-repo")
                        .class("cv-btn cv-primary")
                        .text(env.t.t("ui_repos_create")),
                ),
            );
        panel(div().text(env.t.t("ui_repos_add_title")))
            .attr("id", "add-repo")
            .child(div().class("cv-meta-list").child(form))
            .into_view()
    })
}

// ── Edit ─────────────────────────────────────────────────────────────────────

pub(crate) fn edit(env: &Env, state: &State, owner: &str, name: &str) -> View {
    // Rebuilt when the repository changes (a save), not when something else does.
    let found = {
        let (state, owner, name) = (state.clone(), owner.to_string(), name.to_string());
        create_memo(move || find(&state, &owner, &name))
    };
    let access = {
        let access = state.access.clone();
        create_memo(move || access.get())
    };
    let projects = {
        let projects = state.projects.clone();
        create_memo(move || projects.get())
    };
    let (env, state) = (env.clone(), state.clone());
    View::dynamic(move || {
        let list_back = back(&env, &env.path("/repos"), "ui_repos_back");
        let page = div()
            .attr("id", "edit")
            .class("cv-page cv-narrow")
            .child(list_back);
        match found.get() {
            None => page.child(waiting(&env)).into_view(),
            Some(None) => page
                .child(nothing(&env, "ui_repos_not_found", "not-found"))
                .into_view(),
            Some(Some(repo)) => {
                let (Some(access), Some(projects)) = (access.get(), projects.get()) else {
                    return page.child(waiting(&env)).into_view();
                };
                edit_form(
                    &env,
                    &state,
                    page,
                    &repo,
                    &access.write,
                    &projects,
                    access.can_write(&repo.project_id),
                )
            }
        }
    })
}

fn edit_form(
    env: &Env,
    state: &State,
    page: Element,
    repo: &Repo,
    writable: &[String],
    projects: &[Project],
    can_edit: bool,
) -> View {
    let owner = Field::new("owner", &repo.owner);
    let name = Field::new("name", &repo.name);
    let clone_url = Field::new("clone_url", &repo.clone_url);
    let branch = Field::new("default_branch", &repo.default_branch);
    let project = Field::new("project_id", &repo.project_id);
    let enabled = create_signal(repo.enabled);

    // Only the projects the caller can write to; a read-only view shows the current one.
    let options = if can_edit {
        project_options(projects, writable)
    } else {
        vec![(
            repo.project_id.clone(),
            project_label(projects, &repo.project_id),
        )]
    };
    let disabled = !can_edit;
    let lock = |element: Element| {
        if disabled {
            element.attr("disabled", "disabled")
        } else {
            element
        }
    };

    let send = {
        let (ask, notice, id) = (Rc::clone(&env.ask), state.notice.clone(), repo.id.clone());
        let (owner, name, clone_url, branch, project, enabled) = (
            owner.clone(),
            name.clone(),
            clone_url.clone(),
            branch.clone(),
            project.clone(),
            enabled.clone(),
        );
        move || {
            let draft = RepoDraft {
                owner: owner.value_untracked(),
                name: name.value_untracked(),
                clone_url: clone_url.value_untracked(),
                default_branch: branch.value_untracked(),
                project_id: project.value_untracked(),
                provider: String::new(),
                enabled: enabled.get_untracked(),
            };
            match draft.to_update() {
                Ok(body) => {
                    notice.set(None);
                    ask(Request::SaveRepo {
                        id: id.clone(),
                        body,
                    });
                }
                Err(why) => notice.set(Some(Notice::ErrKey(why.key().to_string()))),
            }
        }
    };
    let enabled_box = {
        let (on, toggle) = (enabled.clone(), enabled.clone());
        lock(
            input()
                .attr("type", "checkbox")
                .attr("id", "enabled")
                .attr_bool("checked", move || on.get())
                .on_click(move || toggle.update(|v| *v = !*v)),
        )
    };
    let mut fields = div()
        .child(labelled(
            "owner",
            &env.t.t("ui_repos_owner"),
            lock(text_box(&owner)),
        ))
        .child(labelled(
            "name",
            &env.t.t("ui_repos_name"),
            lock(text_box(&name)),
        ))
        .child(labelled(
            "clone_url",
            &env.t.t("ui_repos_clone_url"),
            lock(text_box(&clone_url)),
        ))
        .child(labelled(
            "default_branch",
            &env.t.t("ui_repos_branch"),
            lock(text_box(&branch)),
        ))
        .child(labelled(
            "project_id",
            &env.t.t("ui_repos_project"),
            lock(choice(&project, options)),
        ))
        .child(
            div().class("cv-checkbox-row").child(enabled_box).child(
                label()
                    .attr("for", "enabled")
                    .text(env.t.t("ui_repo_enabled")),
            ),
        );
    if can_edit {
        fields = fields.child(
            div().class("cv-actions-row").child(
                button()
                    .attr("type", "submit")
                    .attr("id", "save-repo")
                    .class("cv-btn cv-primary")
                    .text(env.t.t("ui_repos_save")),
            ),
        );
    }
    let title = div().text(repo.slug());
    let body = if can_edit {
        div().class("cv-meta-list").child(
            form()
                .attr("id", "edit-repo")
                .class("cv-form")
                .on_submit(send)
                .child(fields),
        )
    } else {
        div()
            .class("cv-meta-list")
            .child(
                p().attr("id", "view-only")
                    .class("cv-hint")
                    .text(env.t.t("ui_repos_view_only")),
            )
            .child(fields)
    };
    let mut page = page.child(panel(title).child(body));
    if can_edit {
        let (ask, id) = (Rc::clone(&env.ask), repo.id.clone());
        page = page.child(
            div()
                .class("cv-panel cv-danger")
                .child(
                    div()
                        .class("cv-panel-title")
                        .text(env.t.t("ui_repos_delete_title")),
                )
                .child(
                    div()
                        .class("cv-meta-list")
                        .child(p().class("cv-hint").text(env.t.t("ui_repos_delete_hint")))
                        .child(
                            button()
                                .attr("type", "button")
                                .attr("id", "delete-repo")
                                .class("cv-btn cv-delete")
                                .text(env.t.t("ui_repos_delete"))
                                .on_click(move || ask(Request::DeleteRepo(id.clone()))),
                        ),
                ),
        );
    }
    page.into_view()
}

// ── Run with parameters ──────────────────────────────────────────────────────

pub(crate) fn launch(env: &Env, state: &State, owner: &str, name: &str) -> View {
    let found = {
        let (state, owner, name) = (state.clone(), owner.to_string(), name.to_string());
        create_memo(move || find(&state, &owner, &name))
    };
    let stages = {
        let stages = state.stages.clone();
        create_memo(move || stages.get())
    };
    let access = {
        let access = state.access.clone();
        create_memo(move || access.get())
    };
    let (env, state) = (env.clone(), state.clone());
    View::dynamic(move || {
        let page = div().attr("id", "launch").class("cv-page cv-narrow");
        let repo = match found.get() {
            None => return page.child(waiting(&env)).into_view(),
            Some(None) => {
                return page
                    .child(nothing(&env, "ui_scan_repo_not_found", "not-found"))
                    .into_view();
            }
            Some(Some(repo)) => repo,
        };
        let known = match stages.get() {
            Some((id, known)) if id == repo.id => known,
            _ => return page.child(waiting(&env)).into_view(),
        };
        let can_run = access.get().is_some_and(|a| a.can_write(&repo.project_id));
        launch_form(&env, &state, page, &repo, &known, can_run)
    })
}

fn launch_form(
    env: &Env,
    state: &State,
    page: Element,
    repo: &Repo,
    known: &[conveyor_core::KnownStage],
    can_run: bool,
) -> View {
    let git_ref = Field::new("git_ref", "");
    let extra = Field::new("extra", "");
    let ticks: Vec<(String, Signal<bool>)> = known
        .iter()
        .map(|s| (s.name.clone(), create_signal(s.on_by_default)))
        .collect();

    let send = {
        let (ask, notice, id) = (Rc::clone(&env.ask), state.notice.clone(), repo.id.clone());
        let (git_ref, extra, ticks) = (git_ref.clone(), extra.clone(), ticks.clone());
        move || {
            let draft = LaunchDraft {
                git_ref: git_ref.value_untracked(),
                ticked: ticks
                    .iter()
                    .filter(|(_, on)| on.get_untracked())
                    .map(|(name, _)| name.clone())
                    .collect(),
                extra: extra.value_untracked(),
            };
            match draft.to_trigger() {
                Ok(body) => {
                    notice.set(None);
                    ask(Request::RunStages {
                        repo: id.clone(),
                        body,
                    });
                }
                Err(why) => notice.set(Some(Notice::ErrKey(why.key().to_string()))),
            }
        }
    };

    let mut boxes = div().class("cv-stages");
    if known.is_empty() {
        boxes = boxes.child(
            p().attr("id", "none-known")
                .class("cv-hint")
                .text(env.t.t("ui_launch_none_known")),
        );
    }
    for (index, stage) in known.iter().enumerate() {
        let id = format!("stage-{index}");
        let (on, toggle) = (ticks[index].1.clone(), ticks[index].1.clone());
        boxes = boxes.child(
            div()
                .class("cv-stage-pick")
                .child(
                    input()
                        .attr("type", "checkbox")
                        .attr("id", id.clone())
                        .attr("name", "stage")
                        .attr("value", stage.name.clone())
                        .attr_bool("checked", move || on.get())
                        .on_click(move || toggle.update(|v| *v = !*v)),
                )
                .child(label().attr("for", id).text(stage.name.clone()))
                .child((!stage.on_by_default).then(|| {
                    span()
                        .class("cv-muted")
                        .text(env.t.t("ui_launch_normally_skipped"))
                })),
        );
    }

    let mut submit = button()
        .attr("type", "submit")
        .attr("id", "launch-submit")
        .class("cv-btn cv-primary")
        .text(env.t.t("ui_launch_submit"));
    let mut form = form()
        .attr("id", "launch-form")
        .class("cv-form")
        .on_submit(send)
        .child(labelled(
            "git_ref",
            &env.t.t("ui_launch_ref"),
            text_box(&git_ref).attr("placeholder", repo.default_branch.clone()),
        ))
        .child(h3().text(env.t.t("ui_launch_stages")))
        .child(boxes)
        .child(labelled(
            "extra",
            &env.t.t("ui_launch_extra"),
            text_box(&extra).attr("placeholder", "security, lint"),
        ));
    if !can_run {
        submit = submit.attr("disabled", "disabled");
        form = form.child(
            p().attr("id", "no-access")
                .class("cv-hint")
                .text(env.t.t("ui_launch_no_access")),
        );
    }
    form = form.child(div().class("cv-actions-row").child(submit));

    page.child(back(env, &env.repo_path(repo, "/scan"), "ui_scan_back"))
        .child(
            panel(div().text(repo.slug()))
                .child(p().class("cv-hint").text(env.t.t("ui_launch_subtitle")))
                .child(div().class("cv-meta-list").child(form)),
        )
        .into_view()
}
