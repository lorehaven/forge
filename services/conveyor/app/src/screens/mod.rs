//! The screens, as functions from the state to a view.

use std::rc::Rc;

use conveyor_core::tree::ancestor_chain;
use conveyor_core::{Project, Repo, Run, Status, format};
use quench_core::time::Time;
use quench_ui::i18n::I18n;
use quench_ui::prelude::*;
use quench_ui::router::link;

use crate::live::{Problem, Request};

mod credentials;
mod home;
mod repos;
mod runs;
mod scan;

pub(crate) use credentials::credentials;
pub(crate) use home::home;
pub(crate) use repos::{edit, launch, repos};
pub(crate) use runs::{history, run};
pub(crate) use scan::{check, scan};

/// What every screen needs of the app.
#[derive(Clone)]
pub struct Env {
    pub t: Rc<I18n>,
    pub base: String,
    pub ask: Rc<dyn Fn(Request)>,
    pub now: Rc<dyn Fn() -> Time>,
    pub login: String,
}

impl Env {
    pub(crate) fn path(&self, rest: &str) -> String {
        format!("{}/ui{rest}", self.base)
    }

    pub(crate) fn tr(&self, key: &str, args: &[(&str, quench_ui::i18n::Value)]) -> String {
        self.t.tr(key, args)
    }

    /// `/ui/repos/{owner}/{name}{rest}`.
    pub(crate) fn repo_path(&self, repo: &Repo, rest: &str) -> String {
        self.path(&format!(
            "/repos/{}/{}{rest}",
            format::segment(&repo.owner),
            format::segment(&repo.name)
        ))
    }

    pub(crate) fn since(&self, when: Time) -> String {
        format::relative(when, (self.now)())
    }
}

pub(crate) fn waiting(env: &Env) -> View {
    p().attr("id", "waiting")
        .class("fui-empty")
        .text(env.t.t("ui_loading"))
        .into_view()
}

/// A line that says there is nothing here.
pub(crate) fn nothing(env: &Env, key: &str, id: &str) -> Element {
    p().attr("id", id.to_string())
        .class("fui-empty")
        .text(env.t.t(key))
}

#[must_use]
pub fn not_found(env: &Env) -> View {
    div()
        .class("cv-page")
        .child(nothing(env, "ui_not_found", "not-found"))
        .child(link(&env.path("/home")).text(env.t.t("ui_home_button")))
        .into_view()
}

/// Whether the service said there is no such thing.
pub(crate) fn is_missing(problem: &Signal<Option<Problem>>) -> bool {
    problem.get() == Some(Problem::NotFound)
}

pub(crate) fn panel(title: Element) -> Element {
    div()
        .class("cv-panel")
        .child(div().class("cv-panel-title").child(title))
}

pub(crate) fn pill(env: &Env, status: Status) -> Element {
    span()
        .class(format!("cv-status cv-status-{status}"))
        .text(env.t.t(&format!("ui_status_{status}")))
}

pub(crate) fn enabled_badge(env: &Env, enabled: bool) -> Element {
    if enabled {
        span()
            .class("cv-status cv-status-success")
            .text(env.t.t("ui_repo_enabled"))
    } else {
        span()
            .class("cv-status cv-status-skipped")
            .text(env.t.t("ui_repo_disabled"))
    }
}

/// `Home / root / ... / project`, each segment but the last a link back up the tree.
pub(crate) fn breadcrumb(env: &Env, all: &[Project], project: &Project) -> Element {
    let chain = ancestor_chain(&project.id, all);
    let mut nav = h3()
        .attr("id", "breadcrumb")
        .class("cv-breadcrumb")
        .child(link(&env.path("/home")).text(env.t.t("ui_home_button")));
    let last = chain.len().saturating_sub(1);
    for (index, node) in chain.iter().enumerate() {
        nav = nav.child(span().class("cv-breadcrumb-sep").text("/"));
        nav = nav.child(if index == last {
            span()
                .class("cv-breadcrumb-current")
                .text(node.name.clone())
        } else {
            link(&env.path(&format!("/projects/{}", node.id))).text(node.name.clone())
        });
    }
    nav
}

/// The runs, newest first, as the front page and the history show them.
pub(crate) fn runs_table(env: &Env, runs: &[Run], repos: &[Repo]) -> Element {
    if runs.is_empty() {
        return nothing(env, "ui_runs_empty", "no-runs");
    }
    let mut head = element("tr");
    for key in [
        "ui_col_status",
        "ui_col_repository",
        "ui_col_ref",
        "ui_col_commit",
        "ui_col_trigger",
        "ui_col_when",
    ] {
        head = head.child(element("th").text(env.t.t(key)));
    }
    let mut table = element("table")
        .attr("id", "runs")
        .class("cv-table")
        .child(head);
    for run in runs {
        let slug = repos
            .iter()
            .find(|r| r.id == run.repo_id)
            .map_or_else(|| "-".to_string(), Repo::slug);
        table = table.child(
            element("tr")
                .attr("data-run", run.id.clone())
                .child(element("td").child(pill(env, run.status)))
                .child(
                    element("td").child(
                        link(&env.path(&format!("/runs/{}", run.id)))
                            .class("cv-run-link")
                            .text(slug),
                    ),
                )
                .child(
                    element("td")
                        .class("cv-mono")
                        .text(run.ref_name().to_string()),
                )
                .child(
                    element("td")
                        .class("cv-mono cv-muted")
                        .text(run.short_sha().to_string()),
                )
                .child(
                    element("td")
                        .class("cv-muted")
                        .text(run.trigger.to_string()),
                )
                .child(
                    element("td")
                        .class("cv-muted")
                        .text(env.since(run.queued_at)),
                ),
        );
    }
    div().class("cv-scroll").child(table)
}

pub(crate) fn labelled(id: &str, text: &str, control: Element) -> Element {
    div()
        .class("cv-field")
        .child(label().attr("for", id.to_string()).text(text.to_string()))
        .child(control)
}

pub(crate) fn back(env: &Env, href: &str, key: &str) -> Element {
    link(href)
        .attr("id", "back")
        .class("cv-btn cv-back")
        .text(env.t.t(key))
}

pub(crate) fn project_label(all: &[Project], id: &str) -> String {
    conveyor_core::tree::project_path(id, all)
}
