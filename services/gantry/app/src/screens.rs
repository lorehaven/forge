//! The screens, as functions from the state to a view.

use std::rc::Rc;

use gantry_core::filter::{chips, matching};
use gantry_core::format::when;
use gantry_core::model::RESOURCE_STATES;
use gantry_core::{DeploymentView, EditDraft, Group, ResourceRef, Row};
use quench_ui::form::Field;
use quench_ui::i18n::{I18n, Value};
use quench_ui::prelude::*;
use quench_ui::router::{Router, link};

use crate::live::{Notice, Pending, Problem, Request, State};

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

    fn tr(&self, key: &str, args: &[(&str, Value)]) -> String {
        self.t.tr(key, args)
    }
}

fn waiting(env: &Env) -> View {
    p().attr("id", "waiting")
        .class("gt-empty")
        .text(env.t.t("ui_loading"))
        .into_view()
}

#[must_use]
pub fn not_found(env: &Env) -> View {
    div()
        .class("gt-page")
        .child(
            p().attr("id", "not-found")
                .class("gt-empty")
                .text(env.t.t("ui_not_found")),
        )
        .child(link(&env.path("/home")).text(env.t.t("ui_nav_targets")))
        .into_view()
}

/// Packages and History in the side panel, the open one marked.
#[must_use]
pub(crate) fn sidebar(env: &Env, router: &Router) -> View {
    let (env, router) = (env.clone(), router.clone());
    View::dynamic(move || {
        let here = router.location().path;
        let packages = env.path("/home");
        let history = env.path("/operations");
        let in_history = here.starts_with(&history);
        let in_packages = !in_history && here.starts_with(&env.path(""));
        let mut first = link(&packages)
            .class("fui-nav-item")
            .attr("id", "nav-packages")
            .child(forge_shell::icon("box"))
            .child(span().text(env.t.t("ui_nav_targets")));
        let mut second = link(&history)
            .class("fui-nav-item")
            .attr("id", "nav-history")
            .child(forge_shell::icon("clock"))
            .child(span().text(env.t.t("ui_nav_operations")));
        if in_packages {
            first = first.attr("aria-current", "page").class("gt-current");
        }
        if in_history {
            second = second.attr("aria-current", "page").class("gt-current");
        }
        div().child(first).child(second).into_view()
    })
}

/// Above the page: why there is no data, what a change said, a change waiting for a yes.
#[must_use]
pub(crate) fn banner(env: &Env, state: &State, login_url: &str) -> View {
    let (problem, notice, pending) = (
        state.problem.clone(),
        state.notice.clone(),
        state.pending.clone(),
    );
    let (env, login) = (env.clone(), login_url.to_string());
    let ask = Rc::clone(&env.ask);
    View::dynamic(move || {
        let problem_view = match problem.get() {
            Some(Problem::Failed(why)) => p()
                .attr("id", "problem")
                .class("gt-notice gt-notice-error")
                .text(why)
                .into_view(),
            Some(Problem::SignedOut) => p()
                .attr("id", "problem")
                .class("gt-notice gt-notice-error")
                .child(span().text(format!("{} ", env.t.t("ui_session_ended"))))
                .child(
                    a().attr("id", "sign-in")
                        .attr("href", login.clone())
                        .text(env.t.t("ui_sign_in")),
                )
                .into_view(),
            _ => View::Empty,
        };
        let notice_view = notice.get().map_or(View::Empty, |Notice { ok, text }| {
            p().attr("id", "notice")
                .class(if ok {
                    "gt-notice gt-notice-ok"
                } else {
                    "gt-notice gt-notice-error"
                })
                .text(text)
                .into_view()
        });
        let pending_view = pending
            .get()
            .map_or(View::Empty, |Pending { question, request }| {
                let (yes_pending, yes_ask) = (pending.clone(), Rc::clone(&ask));
                let no_pending = pending.clone();
                div()
                    .attr("id", "confirm")
                    .class("gt-notice gt-notice-info gt-confirm")
                    .child(span().attr("id", "confirm-question").text(question))
                    .child(
                        button()
                            .attr("type", "button")
                            .attr("id", "confirm-yes")
                            .class("gt-btn gt-btn-primary")
                            .on_click(move || {
                                yes_pending.set(None);
                                yes_ask(request.clone());
                            })
                            .text(env.t.t("ui_confirm_yes")),
                    )
                    .child(
                        button()
                            .attr("type", "button")
                            .attr("id", "confirm-no")
                            .class("gt-btn")
                            .on_click(move || no_pending.set(None))
                            .text(env.t.t("ui_cancel")),
                    )
                    .into_view()
            });
        fragment([problem_view, notice_view, pending_view])
    })
}

/// A coloured dot and a translated word.
fn badge(env: &Env, kind: &str, value: &str) -> Element {
    span()
        .class(format!("gt-state gt-state-{value}"))
        .attr("data-state", value.to_string())
        .child(span().class("gt-dot"))
        .child(span().text(env.t.t(&format!("ui_{kind}_{value}"))))
}

/// A button that asks the host for `request` when it is pressed.
fn act(env: &Env, id: &str, label: &str, style: &str, request: Request) -> Element {
    let ask = Rc::clone(&env.ask);
    button()
        .attr("type", "button")
        .attr("id", id.to_string())
        .class(if style.is_empty() {
            "gt-btn".to_string()
        } else {
            format!("gt-btn gt-btn-{style}")
        })
        .on_click(move || ask(request.clone()))
        .text(label.to_string())
}

/// A button that asks the person first. The question names the thing, so it is not a reflex "are you sure".
fn act_confirmed(
    _env: &Env,
    state: &State,
    id: &str,
    label: &str,
    style: &str,
    question: String,
    request: Request,
) -> Element {
    let pending = state.pending.clone();
    button()
        .attr("type", "button")
        .attr("id", id.to_string())
        .class(if style.is_empty() {
            "gt-btn".to_string()
        } else {
            format!("gt-btn gt-btn-{style}")
        })
        .on_click(move || {
            pending.set(Some(Pending {
                question: question.clone(),
                request: request.clone(),
            }));
        })
        .text(label.to_string())
}

fn is_not_found(state: &State) -> bool {
    state.problem.get() == Some(Problem::NotFound)
}

fn section(env: &Env, key: &str) -> Element {
    div().class("gt-section").text(env.t.t(key))
}

fn head_cell(env: &Env, key: &str) -> Element {
    element("th").text(if key.is_empty() {
        String::new()
    } else {
        env.t.t(key)
    })
}

fn table_head(env: &Env, keys: &[&str]) -> Element {
    let mut row = element("tr");
    for key in keys {
        row = row.child(head_cell(env, key));
    }
    element("thead").child(row)
}

// ── Home ─────────────────────────────────────────────────────────────────────

fn card(env: &Env, group: &Group) -> View {
    let summary = &group.summary;
    let mut notes = div().class("gt-app-notes");
    for (n, key) in [
        (summary.missing, "ui_rstate_missing"),
        (summary.edited, "ui_rstate_edited"),
        (summary.extra, "ui_rstate_extra"),
        (summary.not_ready, "ui_app_not_ready"),
    ] {
        if n > 0 {
            notes = notes.child(
                span()
                    .class("gt-note")
                    .text(format!("{n} {}", env.t.t(key))),
            );
        }
    }
    let version = group.installed.clone().unwrap_or_else(|| "—".to_string());
    link(&env.path(&format!("/apps/{}", group.package)))
        .class("gt-app")
        .attr("data-package", group.package.clone())
        .child(
            div()
                .class("gt-app-head")
                .child(strong().class("gt-app-name").text(group.package.clone()))
                .child(badge(env, "sync", &summary.sync)),
        )
        .child(
            p().class("gt-app-version")
                .child(span().text(version))
                .child(
                    summary
                        .update_to
                        .as_ref()
                        .map(|latest| span().class("gt-update").text(format!(" → {latest}"))),
                ),
        )
        .child(
            group
                .description
                .clone()
                .map(|text| p().class("gt-desc gt-app-desc").text(text)),
        )
        .child(
            div()
                .class("gt-app-foot")
                .child(span().class("gt-muted").text(format!(
                    "{} {}",
                    summary.total,
                    env.t.t("ui_app_resources")
                )))
                .child(notes),
        )
        .into_view()
}

/// Everything a card shows, so that a card is rebuilt when (and only when) one of them changes.
fn card_key(group: &Group) -> String {
    format!(
        "{}|{:?}|{:?}|{:?}",
        group.package, group.summary, group.installed, group.description
    )
}

#[must_use]
pub(crate) fn home(env: &Env, state: &State) -> View {
    let name = Field::new("home-q", "");
    let sync_filter: Signal<String> = create_signal(String::new());

    let busy = {
        let (env, busy) = (env.clone(), state.busy.clone());
        View::dynamic(move || {
            busy.get().map_or(View::Empty, |op| {
                p().attr("id", "busy")
                    .class("gt-notice gt-notice-info")
                    .child(
                        link(&env.path(&format!("/operations/{}", op.id)))
                            .text(format!("{} …", op.title)),
                    )
                    .into_view()
            })
        })
    };

    let chip_row = {
        let (env, groups, sync) = (env.clone(), state.groups.clone(), sync_filter.clone());
        View::dynamic(move || {
            let chosen = sync.get();
            let all = groups.get().unwrap_or_default();
            let mut row = div()
                .class("gt-chips")
                .child(chip(&env, &sync, "", all.len()));
            for (state, n) in chips(&all, &chosen) {
                row = row.child(chip(&env, &sync, state, n));
            }
            row.into_view()
        })
    };

    let filter = div()
        .class("gt-form gt-inline-form gt-filter")
        .child(chip_row)
        .child(
            name.input()
                .class("gt-input")
                .attr("type", "text")
                .attr("placeholder", env.t.t("ui_filter_name")),
        );

    let message = {
        let (env, state, name, sync_filter) = (
            env.clone(),
            state.clone(),
            name.clone(),
            sync_filter.clone(),
        );
        View::dynamic(move || {
            let Some(total) = state.groups.with(|g| g.as_ref().map(Vec::len)) else {
                return if state.problem.get().is_some() {
                    View::Empty
                } else {
                    waiting(&env)
                };
            };
            if total == 0 {
                return p()
                    .attr("id", "no-packages")
                    .class("gt-empty")
                    .text(env.t.t("ui_home_no_targets"))
                    .into_view();
            }
            let shown = state.groups.with(|g| {
                matching(
                    g.as_deref().unwrap_or_default(),
                    &sync_filter.get(),
                    &name.value(),
                )
            });
            if shown.is_empty() {
                p().attr("id", "no-match")
                    .class("gt-muted")
                    .text(env.t.t("ui_filter_no_match"))
                    .into_view()
            } else {
                View::Empty
            }
        })
    };

    let grid = {
        let (env, groups, sync) = (env.clone(), state.groups.clone(), sync_filter.clone());
        View::keyed(
            move || {
                groups.with(|g| {
                    matching(g.as_deref().unwrap_or_default(), &sync.get(), &name.value())
                })
            },
            card_key,
            move |group| card(&env, &group),
        )
    };

    div()
        .class("gt-page")
        .child(busy)
        .child(filter)
        .child(message)
        .child(div().class("gt-grid").attr("id", "packages").child(grid))
        .into_view()
}

/// A button that narrows the list to one sync state (empty: every package).
fn chip(env: &Env, sync: &Signal<String>, state: &'static str, n: usize) -> Element {
    let label = if state.is_empty() {
        env.t.t("ui_filter_all")
    } else {
        env.t.t(&format!("ui_sync_{state}"))
    };
    let (set, active) = (sync.clone(), sync.clone());
    button()
        .attr("type", "button")
        .attr(
            "id",
            format!("chip-{}", if state.is_empty() { "all" } else { state }),
        )
        .class("gt-chip")
        .class_if("gt-chip-active", move || active.get() == state)
        .on_click(move || set.set(state.to_string()))
        .child(span().text(label))
        .child(span().class("gt-muted").text(format!(" {n}")))
}

// ── One package ──────────────────────────────────────────────────────────────

/// The actions on the whole package.
fn group_actions(env: &Env, state: &State, group: &Group) -> Element {
    let package = group.package.clone();
    let mut bar = div().class("gt-group-actions");
    if let (true, Some(latest)) = (group.may("deploy"), &group.summary.update_to) {
        bar = bar.child(act_confirmed(
            env,
            state,
            "upgrade",
            &env.t.t("ui_action_upgrade"),
            "primary",
            env.tr(
                "ui_confirm_upgrade",
                &[
                    ("package", package.clone().into()),
                    ("version", latest.clone().into()),
                ],
            ),
            Request::Sync {
                package: package.clone(),
                version: Some(latest.clone()),
            },
        ));
    }
    if group.may("scale") && group.summary.missing > 0 {
        bar = bar.child(act(
            env,
            "apply-missing",
            &env.t.t("ui_action_apply_missing"),
            "",
            Request::ApplyMissing(package.clone()),
        ));
    }
    if !group.inventoried && group.installed.is_some() {
        bar = bar.child(act(
            env,
            "refresh",
            &env.t.t("ui_action_refresh"),
            "",
            Request::Refresh(package.clone()),
        ));
    }
    bar.child(
        link(&env.path(&format!("/targets/{package}")))
            .attr("id", "versions")
            .class("gt-btn")
            .text(env.t.t("ui_action_versions")),
    )
}

/// The package's declared deployments, with the stop/start actions on each.
fn deployments_panel(
    env: &Env,
    state: &State,
    views: &[DeploymentView],
    may_scale: bool,
) -> Element {
    let mut deps = div().class("gt-deps").attr("id", "deployments");
    for view in views {
        let mut buttons = div().class("gt-right");
        if may_scale && view.installed() {
            if view.can_stop() {
                buttons = buttons.child(act_confirmed(
                    env,
                    state,
                    &format!("stop-{}", view.name),
                    &env.t.t("ui_action_stop"),
                    "",
                    env.tr("ui_confirm_stop", &[("name", view.name.clone().into())]),
                    Request::Stop(view.name.clone()),
                ));
            }
            if view.can_start() {
                buttons = buttons.child(act(
                    env,
                    &format!("start-{}", view.name),
                    &env.t.t("ui_action_start"),
                    "",
                    Request::Start(view.name.clone()),
                ));
            }
        }
        deps = deps.child(
            div()
                .class("gt-dep")
                .attr("data-deployment", view.name.clone())
                .child(badge(env, "observed", &view.observed))
                .child(strong().text(view.name.clone()))
                .child((!view.conflicts_with.is_empty()).then(|| {
                    span()
                        .class("gt-desc")
                        .text(format!("⇄ {}", view.conflicts_with.join(", ")))
                }))
                .child(buttons),
        );
    }
    deps
}

/// Counts that are also the quickest filters.
fn state_counts(env: &Env, status: &Field, group: &Group) -> Element {
    let summary = &group.summary;
    let mut counts = div().class("gt-counts");
    for (name, n) in [
        ("synced", summary.synced),
        ("edited", summary.edited),
        ("missing", summary.missing),
        ("extra", summary.extra),
    ] {
        if n == 0 {
            continue;
        }
        let pick = status.clone();
        counts = counts.child(
            button()
                .attr("type", "button")
                .attr("id", format!("count-{name}"))
                .class("gt-count")
                .on_click(move || pick.set(name))
                .child(badge(env, "rstate", name))
                .child(span().class("gt-muted").text(format!(" {n}"))),
        );
    }
    counts
}

/// What the table needs of one row: the row, the package, and what the caller may do.
#[derive(Clone)]
struct Line {
    package: String,
    row: Row,
    may_scale: bool,
    may_deploy: bool,
}

impl Line {
    fn key(&self) -> String {
        let r = &self.row;
        format!(
            "{}|{}|{}|{:?}|{}|{:?}|{}|{}|{}",
            self.package,
            r.kind,
            r.name,
            r.namespace,
            r.state,
            r.ready,
            r.editable,
            self.may_scale,
            self.may_deploy
        )
    }
}

fn resource_row(env: &Env, state: &State, line: &Line) -> View {
    let row = &line.row;
    let reference = ResourceRef::of(&line.package, row);
    let tag = format!("{}-{}", row.kind, row.name);
    let mut buttons = div().class("gt-right");
    if line.may_scale && row.state == "missing" {
        buttons = buttons.child(act(
            env,
            &format!("apply-{tag}"),
            &env.t.t("ui_action_apply"),
            "",
            Request::ApplyResource(reference.clone()),
        ));
    }
    if line.may_deploy && row.editable {
        buttons = buttons.child(
            link(&env.path(&format!("/resource?{}", reference.query())))
                .attr("id", format!("edit-{tag}"))
                .class("gt-btn")
                .text(env.t.t("ui_action_edit")),
        );
    }
    if line.may_scale && !matches!(row.state.as_str(), "missing" | "hidden") {
        buttons = buttons.child(act_confirmed(
            env,
            state,
            &format!("delete-{tag}"),
            &env.t.t("ui_action_delete"),
            "danger",
            env.tr(
                "ui_confirm_delete",
                &[
                    ("kind", row.kind.clone().into()),
                    ("name", row.name.clone().into()),
                ],
            ),
            Request::DeleteResource(reference),
        ));
    }
    let cell = |content: Element| element("td").child(content);
    element("tr")
        .attr("data-resource", tag)
        .child(cell(span().class("gt-muted").text(row.kind.clone())))
        .child(cell(span().text(row.name.clone())))
        .child(cell(
            span()
                .class("gt-muted")
                .text(row.namespace.clone().unwrap_or_default()),
        ))
        .child(cell(
            div().child(badge(env, "rstate", &row.state)).child(
                row.ready
                    .clone()
                    .map(|r| span().class("gt-desc gt-gap").text(r)),
            ),
        ))
        .child(cell(buttons))
        .into_view()
}

/// A `<select>` bound to `field`; `options` are `(value, text)`, and those in `disabled` cannot be picked.
fn select_field(field: &Field, options: Vec<(String, String)>, disabled: &[String]) -> Element {
    let pick = field.clone();
    let rows = options.into_iter().map(|(value, text)| {
        let (field, v) = (field.clone(), value.clone());
        let mut choice = option()
            .attr("value", value.clone())
            .attr_bool("selected", move || field.value() == v)
            .text(text);
        if disabled.contains(&value) {
            choice = choice.attr("disabled", "true");
        }
        choice.into_view()
    });
    select()
        .class("gt-input")
        .attr("id", field.name().to_string())
        .attr("name", field.name().to_string())
        .on_input(move |v| pick.set(v))
        .children(rows)
}

#[allow(clippy::too_many_lines)]
#[must_use]
pub(crate) fn package(env: &Env, state: &State, name: String) -> View {
    let find = {
        let (groups, name) = (state.groups.clone(), name.clone());
        move || {
            groups.with(|g| {
                g.as_ref()
                    .and_then(|g| g.iter().find(|x| x.package == name).cloned())
            })
        }
    };
    // The three fields are the filter: the count buttons and the clear button write them.
    let query = Field::new("app-q", "");
    let kind = Field::new("app-kind", "");
    let status = Field::new("app-state", "");

    let head = {
        let (env, state, find, status) = (env.clone(), state.clone(), find.clone(), status.clone());
        View::dynamic(move || {
            if is_not_found(&state) {
                return not_found(&env);
            }
            let Some(group) = find() else {
                return if state.problem.get().is_some() {
                    View::Empty
                } else {
                    waiting(&env)
                };
            };
            let views: Vec<DeploymentView> = state.deployments.with(|d| {
                d.iter()
                    .filter(|d| d.declared && d.target == group.package)
                    .cloned()
                    .collect()
            });
            div()
                .class("gt-part")
                .child(
                    div()
                        .class("gt-group-head")
                        .child(
                            div()
                                .class("gt-group-title")
                                .child(strong().class("gt-app-name").text(group.package.clone()))
                                .child(badge(&env, "sync", &group.summary.sync))
                                .child(span().class("gt-muted").text(group.versions())),
                        )
                        .child(group_actions(&env, &state, &group)),
                )
                .child(
                    group
                        .description
                        .clone()
                        .map(|text| p().class("gt-desc").text(text)),
                )
                .child(state_counts(&env, &status, &group))
                .child(
                    (!views.is_empty())
                        .then(|| deployments_panel(&env, &state, &views, group.may("scale"))),
                )
                .into_view()
        })
    };

    // The kinds the package has: the select is built again only when they change.
    let kind_select = {
        let (env, find, kind) = (env.clone(), find.clone(), kind.clone());
        View::dynamic(move || {
            let kinds = find().map(|g| g.kinds()).unwrap_or_default();
            let mut options = vec![(String::new(), env.t.t("ui_filter_all_kinds"))];
            options.extend(kinds.into_iter().map(|k| (k.clone(), k)));
            select_field(&kind, options, &[]).into_view()
        })
    };
    let state_select = {
        let mut options = vec![(String::new(), env.t.t("ui_filter_all_states"))];
        options.extend(
            RESOURCE_STATES
                .iter()
                .map(|s| ((*s).to_string(), env.t.t(&format!("ui_rstate_{s}")))),
        );
        select_field(&status, options, &[])
    };
    let clear = {
        let (env, query, kind, status) = (env.clone(), query.clone(), kind.clone(), status.clone());
        View::dynamic(move || {
            let filtering = !(kind.value().is_empty()
                && status.value().is_empty()
                && query.value().trim().is_empty());
            if !filtering {
                return View::Empty;
            }
            let (query, kind, status) = (query.clone(), kind.clone(), status.clone());
            button()
                .attr("type", "button")
                .attr("id", "clear")
                .class("gt-btn")
                .on_click(move || {
                    query.set("");
                    kind.set("");
                    status.set("");
                })
                .text(env.t.t("ui_action_clear"))
                .into_view()
        })
    };
    let tally = {
        let (find, query, kind, status) =
            (find.clone(), query.clone(), kind.clone(), status.clone());
        View::dynamic(move || {
            find().map_or(View::Empty, |g| {
                let shown = g
                    .filtered(&kind.value(), &status.value(), &query.value())
                    .len();
                span()
                    .attr("id", "tally")
                    .class("gt-muted")
                    .text(format!("{shown} / {}", g.rows.len()))
                    .into_view()
            })
        })
    };
    let filter = div()
        .class("gt-form gt-inline-form gt-filter")
        .child(
            query
                .input()
                .class("gt-input")
                .attr("type", "text")
                .attr("placeholder", env.t.t("ui_filter_name")),
        )
        .child(kind_select)
        .child(state_select)
        .child(clear)
        .child(tally);

    let message = {
        let (env, find, query, kind, status) = (
            env.clone(),
            find.clone(),
            query.clone(),
            kind.clone(),
            status.clone(),
        );
        View::dynamic(move || {
            let Some(group) = find() else {
                return View::Empty;
            };
            if group.rows.is_empty() {
                p().attr("id", "no-units")
                    .class("gt-muted")
                    .text(env.t.t("ui_target_no_units"))
                    .into_view()
            } else if group
                .filtered(&kind.value(), &status.value(), &query.value())
                .is_empty()
            {
                p().attr("id", "no-match")
                    .class("gt-muted")
                    .text(env.t.t("ui_filter_no_match"))
                    .into_view()
            } else {
                View::Empty
            }
        })
    };

    let rows = {
        let (env, state, find) = (env.clone(), state.clone(), find);
        View::keyed(
            move || {
                find().map_or_else(Vec::new, |group| {
                    let (may_scale, may_deploy) = (group.may("scale"), group.may("deploy"));
                    group
                        .filtered(&kind.value(), &status.value(), &query.value())
                        .into_iter()
                        .map(|row| Line {
                            package: group.package.clone(),
                            row,
                            may_scale,
                            may_deploy,
                        })
                        .collect()
                })
            },
            Line::key,
            move |line| resource_row(&env, &state, &line),
        )
    };
    let table = element("table")
        .attr("id", "resources")
        .class("gt-table")
        .child(table_head(
            env,
            &[
                "ui_col_kind",
                "ui_col_name",
                "ui_col_namespace",
                "ui_col_status",
                "",
            ],
        ))
        .child(element("tbody").child(rows));

    div()
        .class("gt-page")
        .child(
            p().class("gt-crumb")
                .child(link(&env.path("/home")).text(env.t.t("ui_nav_targets")))
                .child(span().text(format!(" / {name}"))),
        )
        .child(head)
        .child(filter)
        .child(message)
        .child(table)
        .into_view()
}

// ── One target: what runs, what is published ─────────────────────────────────

#[must_use]
pub(crate) fn target(env: &Env, state: &State, name: String) -> View {
    let version = Field::new("gt-version", "");
    let (env, state) = (env.clone(), state.clone());
    View::dynamic(move || {
        if is_not_found(&state) {
            return not_found(&env);
        }
        let Some(target) = state.target.get().filter(|t| t.name == name) else {
            return if state.problem.get().is_some() {
                View::Empty
            } else {
                waiting(&env)
            };
        };
        let mut options = vec![(String::new(), env.t.t("ui_field_version_newest"))];
        options.extend(
            target
                .versions
                .iter()
                .map(|v| (v.version.clone(), v.version.clone())),
        );
        // A version taken back is not offered.
        let yanked: Vec<String> = target
            .versions
            .iter()
            .filter(|v| v.yanked)
            .map(|v| v.version.clone())
            .collect();
        let picker = select_field(&version, options, &yanked);

        let mut units = element("tbody");
        for unit in &target.units {
            units = units.child(
                element("tr")
                    .attr("data-unit", format!("{}/{}", unit.namespace, unit.name))
                    .child(element("td").child(span().class("gt-muted").text(unit.kind.clone())))
                    .child(
                        element("td")
                            .child(span().text(format!("{}/{}", unit.namespace, unit.name))),
                    )
                    .child(element("td").child(
                        span().text(unit.version.clone().unwrap_or_else(|| "—".to_string())),
                    ))
                    .child(
                        element("td")
                            .child(span().text(format!("{}/{}", unit.ready, unit.desired))),
                    ),
            );
        }
        let mut published = element("tbody");
        for v in &target.versions {
            published = published.child(
                element("tr")
                    .attr("data-version", v.version.clone())
                    .child(element("td").child(span().text(v.version.clone())))
                    .child(
                        element("td").child(
                            span()
                                .class("gt-muted")
                                .text(v.description.clone().unwrap_or_default()),
                        ),
                    )
                    .child(element("td").child(if v.yanked {
                        span().class("gt-yanked").text(env.t.t("ui_version_yanked"))
                    } else {
                        span()
                    })),
            );
        }
        let sync = {
            let (ask, package, version) =
                (Rc::clone(&env.ask), target.name.clone(), version.clone());
            move || {
                let chosen = version.value_untracked();
                ask(Request::Sync {
                    package: package.clone(),
                    version: (!chosen.trim().is_empty()).then(|| chosen.trim().to_string()),
                });
            }
        };

        div()
            .class("gt-page")
            .child(
                p().class("gt-crumb")
                    .child(link(&env.path("/home")).text(env.t.t("ui_nav_targets")))
                    .child(
                        link(&env.path(&format!("/apps/{}", target.name)))
                            .text(format!(" / {}", target.name)),
                    ),
            )
            .child(
                div()
                    .class("home-header")
                    .child(h3().text(target.name.clone()))
                    .child(badge(&env, "status", &target.status)),
            )
            .child(
                target
                    .description
                    .clone()
                    .map(|text| p().class("gt-desc").text(text)),
            )
            .child(
                p().attr("id", "versions-line")
                    .class("gt-meta")
                    .text(format!(
                        "{} → {}",
                        target.installed.clone().unwrap_or_else(|| "—".to_string()),
                        target.offered.clone().unwrap_or_else(|| "—".to_string())
                    )),
            )
            .child(
                form()
                    .attr("id", "sync-form")
                    .class("gt-form gt-inline-form")
                    .on_submit(sync)
                    .child(picker)
                    .child(
                        button()
                            .attr("type", "submit")
                            .attr("id", "sync")
                            .class("gt-btn gt-btn-primary")
                            .text(env.t.t("ui_action_sync")),
                    ),
            )
            .child(section(&env, "ui_target_units"))
            .child(if target.units.is_empty() {
                p().attr("id", "no-units")
                    .class("gt-muted")
                    .text(env.t.t("ui_target_no_units"))
                    .into_view()
            } else {
                element("table")
                    .attr("id", "units")
                    .class("gt-table")
                    .child(table_head(
                        &env,
                        &[
                            "ui_col_kind",
                            "ui_col_name",
                            "ui_col_version",
                            "ui_col_ready",
                        ],
                    ))
                    .child(units)
                    .into_view()
            })
            .child(
                element("details")
                    .class("gt-details")
                    .child(element("summary").text(format!(
                        "{} ({})",
                        env.t.t("ui_target_versions"),
                        target.versions.len()
                    )))
                    .child(
                        element("table")
                            .attr("id", "published")
                            .class("gt-table")
                            .child(table_head(
                                &env,
                                &["ui_col_version", "ui_col_description", ""],
                            ))
                            .child(published),
                    ),
            )
            .into_view()
    })
}

// ── Operations ───────────────────────────────────────────────────────────────

#[must_use]
pub(crate) fn operations(env: &Env, state: &State) -> View {
    let (env, state) = (env.clone(), state.clone());
    let list = {
        let (env, operations) = (env.clone(), state.operations.clone());
        View::keyed(
            move || operations.get().unwrap_or_default(),
            |op| format!("{}|{}|{}", op.id, op.state, op.title),
            move |op| {
                element("tr")
                    .attr("data-operation", op.id.clone())
                    .child(element("td").child(
                        link(&env.path(&format!("/operations/{}", op.id))).text(op.title.clone()),
                    ))
                    .child(element("td").child(badge(&env, "state", &op.state)))
                    .child(
                        element("td").child(span().class("gt-muted").text(op.requested_by.clone())),
                    )
                    .child(element("td").child(span().class("gt-muted").text(when(&op.created_at))))
                    .into_view()
            },
        )
    };
    let table = element("table")
        .attr("id", "operations")
        .class("gt-table")
        .child(table_head(
            &env,
            &[
                "ui_col_title",
                "ui_col_state",
                "ui_col_requested_by",
                "ui_col_started",
            ],
        ))
        .child(element("tbody").child(list));
    let body = {
        let (env, operations, problem) =
            (env.clone(), state.operations.clone(), state.problem.clone());
        View::dynamic(
            move || match operations.with(|o| o.as_ref().map(Vec::len)) {
                None if problem.get().is_some() => View::Empty,
                None => waiting(&env),
                Some(0) => p()
                    .attr("id", "no-operations")
                    .class("gt-empty")
                    .text(env.t.t("ui_ops_none"))
                    .into_view(),
                Some(_) => View::Empty,
            },
        )
    };
    div().class("gt-page").child(body).child(table).into_view()
}

#[must_use]
pub(crate) fn operation(env: &Env, state: &State, id: String) -> View {
    let (env, state) = (env.clone(), state.clone());
    View::dynamic(move || {
        if is_not_found(&state) {
            return not_found(&env);
        }
        let Some(op) = state.operation.get().filter(|o| o.id == id) else {
            return if state.problem.get().is_some() {
                View::Empty
            } else {
                waiting(&env)
            };
        };
        let may_cancel = !op.is_terminal() && state.info.with(|i| i.may("deploy"));
        let log = op.log_text();
        let mut steps = element("ol").class("gt-list").attr("id", "steps");
        for line in &op.step_lines {
            steps = steps.child(li().text(line.clone()));
        }
        div()
            .class("gt-page")
            .child(
                div()
                    .class("home-header")
                    .child(h3().attr("id", "operation-title").text(op.title.clone()))
                    .child(badge(&env, "state", &op.state)),
            )
            .child(p().class("gt-meta").text(format!(
                "{} · {}",
                op.requested_by,
                when(&op.created_at)
            )))
            .child(op.error.clone().map(|error| {
                p().attr("id", "operation-error")
                    .class("gt-notice gt-notice-error")
                    .text(error)
            }))
            .child(
                element("details")
                    .class("gt-details")
                    .child(element("summary").text(format!(
                        "{} ({})",
                        env.t.t("ui_plan_steps"),
                        op.step_lines.len()
                    )))
                    .child(steps),
            )
            .child(section(&env, "ui_op_log"))
            .child(
                pre()
                    .attr("id", "log")
                    .class("gt-log")
                    .text(if log.is_empty() {
                        "—".to_string()
                    } else {
                        log
                    }),
            )
            .child(may_cancel.then(|| {
                div().class("gt-bar").child(act(
                    &env,
                    "cancel",
                    &env.t.t("ui_op_cancel"),
                    "danger",
                    Request::Cancel(op.id.clone()),
                ))
            }))
            .into_view()
    })
}

// ── A plan ───────────────────────────────────────────────────────────────────

#[must_use]
pub(crate) fn plan(env: &Env, state: &State, id: String) -> View {
    let (env, state) = (env.clone(), state.clone());
    View::dynamic(move || {
        if is_not_found(&state) {
            return not_found(&env);
        }
        let Some(stored) = state.plan.get().filter(|p| p.id == id) else {
            return if state.problem.get().is_some() {
                View::Empty
            } else {
                waiting(&env)
            };
        };
        let back = if stored.about_deployments() {
            env.path("/home")
        } else {
            env.path(&format!("/targets/{}", stored.target))
        };
        let mut summary = ul().attr("id", "summary");
        for line in &stored.plan.summary {
            summary = summary.child(li().text(line.clone()));
        }
        let mut steps = element("ol").class("gt-list").attr("id", "steps");
        for line in &stored.step_lines {
            steps = steps.child(li().text(line.clone()));
        }
        let bar = if let Some(operation) = stored.confirmed() {
            div().class("gt-bar").child(
                link(&env.path(&format!("/operations/{operation}")))
                    .attr("id", "confirmed")
                    .text(env.t.t("ui_plan_confirmed")),
            )
        } else if stored.can_confirm {
            div()
                .class("gt-bar gt-bar-end")
                .child(link(&back).class("gt-btn").text(env.t.t("ui_back")))
                .child(act(
                    &env,
                    "confirm-plan",
                    &env.t.t("ui_plan_confirm"),
                    "primary",
                    Request::Confirm(stored.id.clone()),
                ))
        } else {
            div().class("gt-bar").child(
                p().attr("id", "not-allowed")
                    .class("gt-muted")
                    .text(env.t.t("ui_plan_not_allowed")),
            )
        };
        div()
            .class("gt-page")
            .child(
                p().class("gt-crumb")
                    .child(link(&back).text(stored.target.clone())),
            )
            .child(
                div()
                    .class("home-header")
                    .child(
                        h3().attr("id", "plan-action")
                            .text(env.t.t(&format!("ui_action_{}", stored.action))),
                    )
                    .child(
                        span()
                            .class("gt-muted")
                            .text(stored.version.clone().unwrap_or_default()),
                    ),
            )
            .child(div().class("gt-card").child(summary))
            .child(
                element("details")
                    .class("gt-details")
                    .child(element("summary").text(format!(
                        "{} ({})",
                        env.t.t("ui_plan_steps"),
                        stored.step_lines.len()
                    )))
                    .child(steps),
            )
            .child(bar)
            .into_view()
    })
}

// ── A resource's YAML ────────────────────────────────────────────────────────

#[must_use]
pub(crate) fn resource(env: &Env, state: &State, resource: ResourceRef) -> View {
    let (env, state) = (env.clone(), state.clone());
    let error: Signal<Option<&'static str>> = create_signal(None);
    View::dynamic(move || {
        if is_not_found(&state) {
            return not_found(&env);
        }
        let Some(yaml) = state.yaml.get() else {
            return if state.problem.get().is_some() {
                View::Empty
            } else {
                waiting(&env)
            };
        };
        // The text typed so far stays in the field when the service refuses it.
        let field = Field::new("yaml", &yaml.yaml);
        let back = env.path(&format!("/apps/{}", resource.package));
        let send = {
            let (ask, resource, field, error) = (
                Rc::clone(&env.ask),
                resource.clone(),
                field.clone(),
                error.clone(),
            );
            move || {
                let draft = EditDraft {
                    resource: resource.clone(),
                    yaml: field.value_untracked(),
                };
                match draft.validate() {
                    Ok(_) => {
                        error.set(None);
                        ask(Request::Edit {
                            resource: draft.resource,
                            yaml: draft.yaml,
                        });
                    }
                    Err(why) => error.set(Some(why.message())),
                }
            }
        };
        let mut area = field
            .bind(textarea())
            .attr("spellcheck", "false")
            .attr("rows", "28")
            .class("gt-yaml");
        if !yaml.editable {
            area = area.attr("readonly", "true");
        }
        div()
            .class("gt-page")
            .child(
                p().class("gt-crumb")
                    .child(link(&back).text(resource.package.clone())),
            )
            .child(
                div()
                    .class("home-header")
                    .child(h3().attr("id", "resource-title").text(resource.title()))
                    .child(span().class("gt-muted").text(resource.namespace.clone())),
            )
            .child(
                form()
                    .attr("id", "edit-form")
                    .class("gt-form")
                    .on_submit(send)
                    .child(area)
                    .child(p().class("gt-desc").text(env.t.t("ui_edit_note")))
                    .child(View::dynamic({
                        let error = error.clone();
                        move || {
                            error.get().map_or(View::Empty, |why| {
                                p().class("gt-error")
                                    .attr("role", "alert")
                                    .text(why)
                                    .into_view()
                            })
                        }
                    }))
                    .child(
                        div()
                            .class("gt-bar gt-bar-end")
                            .child(link(&back).class("gt-btn").text(env.t.t("ui_back")))
                            .child(yaml.editable.then(|| {
                                button()
                                    .attr("type", "submit")
                                    .attr("id", "save")
                                    .class("gt-btn gt-btn-primary")
                                    .text(env.t.t("ui_edit_save"))
                            })),
                    ),
            )
            .into_view()
    })
}
