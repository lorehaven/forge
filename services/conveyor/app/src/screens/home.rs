//! The front page, and a project's branch of it.

use conveyor_core::scan::{CHECKS, chip};
use conveyor_core::tree::{Node, build};
use conveyor_core::{Project, Repo};
use quench_ui::prelude::*;
use quench_ui::router::link;

use super::{Env, breadcrumb, enabled_badge, is_missing, nothing, panel, runs_table, waiting};
use crate::live::{Request, State};

pub(crate) fn home(env: &Env, state: &State, scope: Option<String>) -> View {
    // The whole page is replaced when the project is not there.
    let missing = {
        let (projects, problem, scope) =
            (state.projects.clone(), state.problem.clone(), scope.clone());
        create_memo(move || {
            scope.as_ref().is_some_and(|id| {
                is_missing(&problem)
                    || projects.with(|p| {
                        p.as_ref()
                            .is_some_and(|all| !all.iter().any(|x| &x.id == id))
                    })
            })
        })
    };
    let (env, state) = (env.clone(), state.clone());
    View::dynamic(move || {
        if missing.get() {
            return div()
                .class("cv-page")
                .child(nothing(&env, "ui_project_not_found", "not-found"))
                .into_view();
        }
        div()
            .attr("id", "home")
            .class("cv-page")
            .child(header(&env, &state, scope.clone()))
            .child(runs_panel(&env, &state, scope.clone()))
            .child(tree_panel(&env, &state, scope.clone()))
            .into_view()
    })
}

fn header(env: &Env, state: &State, scope: Option<String>) -> View {
    let (env, projects) = (env.clone(), state.projects.clone());
    View::dynamic(move || {
        let Some(id) = scope.clone() else {
            return div()
                .class("cv-head")
                .child(h1().text(env.t.t("ui_home_title")))
                .child(p().class("cv-subtitle").text(env.t.t("ui_home_subtitle")))
                .into_view();
        };
        let found = projects.with(|all| {
            let all = all.as_deref().unwrap_or_default();
            all.iter()
                .find(|p| p.id == id)
                .map(|project| breadcrumb(&env, all, project))
        });
        div()
            .class("cv-head")
            .child(found)
            .child(
                p().class("cv-subtitle")
                    .text(env.t.t("ui_project_subtitle")),
            )
            .into_view()
    })
}

fn view_all(env: &Env, scope: Option<&String>) -> String {
    scope.map_or_else(
        || env.path("/runs"),
        |id| env.path(&format!("/runs?project={id}")),
    )
}

fn runs_panel(env: &Env, state: &State, scope: Option<String>) -> View {
    let (env, recent, repos) = (env.clone(), state.recent.clone(), state.repos.clone());
    View::dynamic(move || {
        let title = div()
            .class("cv-title-row")
            .child(span().text(env.t.t("ui_runs_title")))
            .child(
                link(&view_all(&env, scope.as_ref()))
                    .attr("id", "view-all")
                    .class("cv-title-link")
                    .text(env.t.t("ui_runs_view_all")),
            );
        let body = recent.with(|runs| {
            runs.as_ref().map(|runs| {
                repos.with(|repos| runs_table(&env, runs, repos.as_deref().unwrap_or_default()))
            })
        });
        panel(title)
            .attr("id", "recent-runs")
            .child(body.map_or_else(|| waiting(&env), IntoView::into_view))
            .into_view()
    })
}

fn tree_panel(env: &Env, state: &State, scope: Option<String>) -> View {
    let (env, state) = (env.clone(), state.clone());
    View::dynamic(move || {
        let title = div()
            .class("cv-title-row")
            .child(span().text(env.t.t("ui_repos_title")))
            .child(
                link(&env.path("/repos"))
                    .attr("id", "manage-repos")
                    .class("cv-title-link")
                    .text(env.t.t("ui_repos_manage")),
            );
        let body = state.projects.with(|projects| {
            state.repos.with(|repos| {
                let (Some(projects), Some(repos)) = (projects.as_ref(), repos.as_ref()) else {
                    return waiting(&env);
                };
                let nodes = build(projects, repos, scope.as_deref());
                if nodes.is_empty() {
                    return nothing(&env, "ui_repos_empty", "no-repos").into_view();
                }
                let mut tree = div().attr("id", "project-tree").class("cv-tree");
                for node in &nodes {
                    tree = tree.child(node_view(&env, &state, node, scope.as_deref()));
                }
                tree.into_view()
            })
        });
        panel(title).child(body).into_view()
    })
}

fn project_link(env: &Env, project: &Project) -> Element {
    link(&env.path(&format!("/projects/{}", project.id)))
        .class("cv-project-name")
        .attr("data-project", project.id.clone())
        .text(project.name.clone())
}

fn node_view(env: &Env, state: &State, node: &Node, scope: Option<&str>) -> View {
    match node {
        Node::Repos(repos) => repo_table(env, state, repos, scope).into_view(),
        Node::Empty(project) => project_link(env, project)
            .class("cv-muted cv-leaf-link")
            .into_view(),
        Node::Container {
            project,
            repos,
            children,
        } => {
            let mut inner = div().class("cv-children");
            for child in children {
                inner = inner.child(node_view(env, state, child, scope));
            }
            let mut details = element("details")
                .class("cv-node")
                .attr("open", "open")
                .child(
                    element("summary")
                        .class("cv-node-head")
                        .child(project_link(env, project)),
                );
            if !repos.is_empty() {
                details = details.child(repo_table(env, state, repos, scope));
            }
            details.child(inner).into_view()
        }
    }
}

fn repo_table(env: &Env, state: &State, repos: &[Repo], scope: Option<&str>) -> Element {
    let mut head = element("tr");
    for key in [
        "ui_col_repository",
        "ui_col_provider",
        "ui_col_branch",
        "ui_col_state",
        "ui_col_checks",
        "ui_col_actions",
    ] {
        head = head.child(element("th").text(env.t.t(key)));
    }
    let mut table = element("table").class("cv-table").child(head);
    for repo in repos {
        table = table.child(repo_row(env, state, repo, scope));
    }
    div().class("cv-scroll").child(table)
}

fn repo_row(env: &Env, state: &State, repo: &Repo, scope: Option<&str>) -> Element {
    let actions = {
        let mut cell = element("td").class("cv-actions");
        // No button at all when disabled: it would just be refused.
        if repo.enabled {
            let (ask, id, scope) = (
                std::rc::Rc::clone(&env.ask),
                repo.id.clone(),
                scope.map(str::to_string),
            );
            cell = cell.child(
                button()
                    .attr("type", "button")
                    .attr("id", format!("run-{}", repo.id))
                    .class("cv-btn cv-run-button")
                    .text(env.t.t("ui_repo_run_now"))
                    .on_click(move || {
                        ask(Request::RunNow {
                            repo: id.clone(),
                            scope: scope.clone(),
                        });
                    }),
            );
        }
        cell.child(
            link(&env.repo_path(repo, "/run"))
                .attr("id", format!("launch-{}", repo.id))
                .class("cv-btn cv-small")
                .text(env.t.t("ui_launch_link")),
        )
    };
    element("tr")
        .attr("data-repo", repo.id.clone())
        .child(
            element("td").child(
                link(&env.repo_path(repo, "/scan"))
                    .class("cv-run-link")
                    .text(repo.name.clone()),
            ),
        )
        .child(
            element("td")
                .class("cv-muted")
                .text(repo.provider.to_string()),
        )
        .child(
            element("td")
                .class("cv-mono")
                .text(repo.default_branch.clone()),
        )
        .child(element("td").child(enabled_badge(env, repo.enabled)))
        .child(element("td").child(chip_row(env, state, &repo.id)))
        .child(actions)
}

/// One chip per check, coloured by what the latest scan found.
fn chip_row(env: &Env, state: &State, repo_id: &str) -> View {
    let (env, chips, id) = (env.clone(), state.chips.clone(), repo_id.to_string());
    View::dynamic(move || {
        let row = chips.with(|all| {
            let mine = all.iter().find(|c| c.repo_id == id);
            let mut row = div().class("cv-chips");
            for (kind, _, _) in CHECKS {
                let found = mine.and_then(|c| c.checks.iter().find(|x| x.kind == kind));
                let chip = chip(kind, found);
                row = row.child(
                    span()
                        .class(format!("cv-chip cv-{}", chip.class))
                        .attr("title", env.t.t(chip.title_key))
                        .attr("data-check", kind)
                        .text(format!("{} {}", chip.letter, chip.label)),
                );
            }
            row
        });
        row.into_view()
    })
}
