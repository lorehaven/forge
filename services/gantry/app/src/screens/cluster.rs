//! The cluster as it is: kinds, a list of one kind, and one object with its events and (for a pod) its log.

use std::rc::Rc;

use gantry_core::format::when;
use gantry_core::model::kinds_by_group;
use gantry_core::{ClusterEntry, ClusterRef};
use quench_ui::form::Field;
use quench_ui::prelude::*;
use quench_ui::router::link;

use super::{
    Env, act_confirmed, is_not_found, not_found, section, select_field, table_head, waiting,
};
use crate::live::{Request, State};

/// A status word, with the dot of its colour: red when it needs attention, green otherwise.
fn status_cell(status: &str, attention: bool) -> Element {
    if status.is_empty() {
        return span().class("gt-muted").text("");
    }
    span()
        .class(if attention {
            "gt-state gt-state-failed"
        } else {
            "gt-state gt-state-succeeded"
        })
        .child(span().class("gt-dot"))
        .child(span().text(status.to_string()))
}

/// A card of the overview: a title, the figure, and a line under it.
fn stat(id: &str, title: &str, figure: String, note: String) -> Element {
    div()
        .class("gt-app")
        .attr("id", id.to_string())
        .child(strong().class("gt-app-name").text(title.to_string()))
        .child(p().class("gt-app-version").text(figure))
        .child(p().class("gt-desc").text(note))
}

fn percent(used: u64, total: u64) -> String {
    (used * 100)
        .checked_div(total)
        .map_or_else(|| "-".to_string(), |share| format!("{share}%"))
}

/// `1.2Gi`: bytes as a person reads them.
#[allow(
    clippy::cast_precision_loss,
    reason = "a figure for reading, not for counting"
)]
fn human_bytes(value: u64) -> String {
    let v = value as f64;
    let (unit, div) = if v >= 1024.0 * 1024.0 * 1024.0 {
        ("Gi", 1024.0 * 1024.0 * 1024.0)
    } else {
        ("Mi", 1024.0 * 1024.0)
    };
    format!("{:.1}{unit}", v / div)
}

/// The numbers of the cluster, and what needs a look.
#[allow(clippy::too_many_lines)]
fn overview(env: &Env, state: &State) -> View {
    let (env, overview) = (env.clone(), state.cluster_overview.clone());
    View::dynamic(move || {
        let Some(o) = overview.get() else {
            return View::Empty;
        };
        let (cpu, memory, cpu_note, memory_note) = if o.metrics {
            (
                format!("{} / {}m", o.cpu_used_m, o.cpu_total_m),
                format!(
                    "{} / {}",
                    human_bytes(o.memory_used),
                    human_bytes(o.memory_total)
                ),
                percent(o.cpu_used_m, o.cpu_total_m),
                percent(o.memory_used, o.memory_total),
            )
        } else {
            (
                format!("{}m", o.cpu_total_m),
                human_bytes(o.memory_total),
                env.t.t("ui_cluster_no_metrics"),
                env.t.t("ui_cluster_no_metrics"),
            )
        };
        let mut page = div().attr("id", "cluster-overview").child(
            div()
                .class("gt-grid")
                .child(stat(
                    "stat-nodes",
                    &env.t.t("ui_kind_Node"),
                    format!("{} / {}", o.nodes_ready, o.nodes_total),
                    env.t.t("ui_cluster_ready"),
                ))
                .child(stat("stat-cpu", &env.t.t("ui_cluster_cpu"), cpu, cpu_note))
                .child(stat(
                    "stat-memory",
                    &env.t.t("ui_cluster_memory"),
                    memory,
                    memory_note,
                ))
                .child(stat(
                    "stat-pods",
                    &env.t.t("ui_kind_Pod"),
                    format!("{} / {}", o.pods_running, o.pods_total),
                    env.t.t("ui_cluster_running"),
                )),
        );
        if !o.pods_attention.is_empty() {
            let mut rows = element("tbody");
            for pod in &o.pods_attention {
                let target = ClusterRef {
                    kind: "Pod".into(),
                    namespace: pod.namespace.clone().unwrap_or_default(),
                    name: pod.name.clone(),
                };
                rows = rows.child(
                    element("tr")
                        .attr("data-attention", pod.name.clone())
                        .child(
                            element("td").child(
                                link(&env.path(&format!("/cluster/Pod/view?{}", target.query())))
                                    .text(pod.name.clone()),
                            ),
                        )
                        .child(
                            element("td")
                                .child(span().class("gt-muted").text(target.namespace.clone())),
                        )
                        .child(element("td").child(status_cell(&pod.status, true))),
                );
            }
            page = page
                .child(section(&env, "ui_cluster_attention"))
                .child(element("table").class("gt-table").child(rows));
        }
        if !o.warnings.is_empty() {
            let mut rows = element("tbody");
            for event in &o.warnings {
                rows = rows.child(
                    element("tr")
                        .attr("data-warning", event.reason.clone())
                        .child(element("td").text(event.object.clone()))
                        .child(element("td").text(event.reason.clone()))
                        .child(
                            element("td")
                                .child(span().class("gt-muted").text(event.message.clone())),
                        )
                        .child(
                            element("td").child(
                                span()
                                    .class("gt-muted")
                                    .text(event.last.as_deref().map(when).unwrap_or_default()),
                            ),
                        ),
                );
            }
            page = page
                .child(section(&env, "ui_cluster_warnings"))
                .child(element("table").class("gt-table").child(rows));
        }
        page.into_view()
    })
}

/// The overview, and the kinds under their headings.
#[must_use]
pub fn cluster_home(env: &Env, state: &State) -> View {
    let numbers = overview(env, state);
    let (env, kinds, problem) = (
        env.clone(),
        state.cluster_kinds.clone(),
        state.problem.clone(),
    );
    let kinds_view = View::dynamic(move || {
        let all = kinds.get();
        if all.is_empty() {
            return if problem.get().is_some() {
                View::Empty
            } else {
                waiting(&env)
            };
        }
        let mut page = div().attr("id", "cluster-kinds");
        for (group, list) in kinds_by_group(&all) {
            let mut row = div().class("gt-chips");
            for kind in list {
                row = row.child(
                    link(&env.path(&format!("/cluster/{}", kind.kind)))
                        .class("gt-chip")
                        .attr("data-kind", kind.kind.clone())
                        .text(env.t.t(&format!("ui_kind_{}", kind.kind))),
                );
            }
            page = page
                .child(section(&env, &format!("ui_cluster_group_{group}")))
                .child(row);
        }
        page.into_view()
    });
    div()
        .class("gt-page")
        .child(numbers)
        .child(kinds_view)
        .into_view()
}

/// Every object of one kind, filtered by a namespace and a piece of the name.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn cluster_list(env: &Env, state: &State, kind: &str) -> View {
    let kind = kind.to_string();
    let name = Field::new("cluster-q", "");
    let namespace = Field::new("cluster-ns", "");

    let namespaces = {
        let (env, list, namespace) = (env.clone(), state.cluster_list.clone(), namespace.clone());
        View::dynamic(move || {
            let mut found: Vec<String> = list.with(|l| {
                l.iter()
                    .flatten()
                    .filter_map(|e| e.namespace.clone())
                    .collect()
            });
            found.sort();
            found.dedup();
            if found.is_empty() {
                return View::Empty;
            }
            let mut options = vec![(String::new(), env.t.t("ui_cluster_all_namespaces"))];
            options.extend(found.into_iter().map(|ns| (ns.clone(), ns)));
            select_field(&namespace, options, &[]).into_view()
        })
    };
    let filter = div()
        .class("gt-form gt-inline-form gt-filter")
        .child(namespaces)
        .child(
            name.input()
                .class("gt-input")
                .attr("type", "text")
                .attr("placeholder", env.t.t("ui_filter_name")),
        );

    let shown = {
        let (list, name, namespace) = (state.cluster_list.clone(), name, namespace);
        move || -> Vec<ClusterEntry> {
            let (q, ns) = (name.value().to_lowercase(), namespace.value());
            list.with(|l| {
                l.iter()
                    .flatten()
                    .filter(|e| ns.is_empty() || e.namespace.as_deref() == Some(ns.as_str()))
                    .filter(|e| q.is_empty() || e.name.to_lowercase().contains(&q))
                    .cloned()
                    .collect()
            })
        }
    };

    let rows = {
        let (env, kind, shown) = (env.clone(), kind.clone(), shown.clone());
        View::keyed(
            shown,
            |e| {
                format!(
                    "{}|{:?}|{}|{}|{}",
                    e.name, e.namespace, e.status, e.detail, e.attention
                )
            },
            move |e| {
                let target = ClusterRef {
                    kind: kind.clone(),
                    namespace: e.namespace.clone().unwrap_or_default(),
                    name: e.name.clone(),
                };
                element("tr")
                    .attr("data-object", e.name.clone())
                    .child(
                        element("td").child(
                            link(&env.path(&format!("/cluster/{}/view?{}", kind, target.query())))
                                .text(e.name.clone()),
                        ),
                    )
                    .child(
                        element("td").child(
                            span()
                                .class("gt-muted")
                                .text(e.namespace.clone().unwrap_or_default()),
                        ),
                    )
                    .child(element("td").child(status_cell(&e.status, e.attention)))
                    .child(element("td").child(span().class("gt-muted").text(e.detail.clone())))
                    .child(
                        element("td").child(
                            span()
                                .class("gt-muted")
                                .text(e.created.as_deref().map(when).unwrap_or_default()),
                        ),
                    )
                    .into_view()
            },
        )
    };
    let table = element("table")
        .attr("id", "cluster-objects")
        .class("gt-table")
        .child(table_head(
            env,
            &[
                "ui_col_name",
                "ui_col_namespace",
                "ui_col_status",
                "ui_col_detail",
                "ui_col_usage",
                "ui_col_created",
            ],
        ))
        .child(element("tbody").child(rows));

    let message = {
        let (env, state, shown) = (env.clone(), state.clone(), shown);
        View::dynamic(move || {
            if state.cluster_list.with(Option::is_none) {
                return if state.problem.get().is_some() {
                    View::Empty
                } else {
                    waiting(&env)
                };
            }
            if shown().is_empty() {
                p().attr("id", "no-objects")
                    .class("fui-empty")
                    .text(env.t.t("ui_cluster_none"))
                    .into_view()
            } else {
                View::Empty
            }
        })
    };

    div()
        .class("gt-page")
        .child(
            div().class("home-header").child(
                h3().attr("id", "cluster-title")
                    .text(env.t.t(&format!("ui_kind_{kind}"))),
            ),
        )
        .child(filter)
        .child(message)
        .child(table)
        .into_view()
}

/// One object: its YAML, its events, and a pod's log.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn cluster_object(env: &Env, state: &State, object: ClusterRef) -> View {
    let (env, state) = (env.clone(), state.clone());
    let container = Field::new("log-container", "");
    let replicas = Field::new("scale-replicas", "1");
    let previous: Signal<bool> = create_signal(false);
    View::dynamic(move || {
        if is_not_found(&state) {
            return not_found(&env);
        }
        let Some(view) = state.cluster_object.get() else {
            return if state.problem.get().is_some() {
                View::Empty
            } else {
                waiting(&env)
            };
        };
        let back = env.path(&format!("/cluster/{}", object.kind));

        let mut events = element("tbody");
        for event in &view.events {
            events = events.child(
                element("tr")
                    .attr("data-event", event.reason.clone())
                    .child(element("td").child(status_cell(&event.kind, event.kind == "Warning")))
                    .child(element("td").text(event.reason.clone()))
                    .child(
                        element("td").child(span().class("gt-muted").text(event.message.clone())),
                    )
                    .child(
                        element("td").child(span().class("gt-muted").text(event.count.to_string())),
                    )
                    .child(
                        element("td").child(
                            span()
                                .class("gt-muted")
                                .text(event.last.as_deref().map(when).unwrap_or_default()),
                        ),
                    ),
            );
        }
        let events_view = if view.events.is_empty() {
            p().class("gt-muted")
                .text(env.t.t("ui_cluster_no_events"))
                .into_view()
        } else {
            element("table")
                .attr("id", "cluster-events")
                .class("gt-table")
                .child(table_head(
                    &env,
                    &[
                        "ui_col_type",
                        "ui_col_reason",
                        "ui_col_message",
                        "ui_col_count",
                        "ui_col_last",
                    ],
                ))
                .child(events)
                .into_view()
        };

        let related = if view.related.is_empty() {
            View::Empty
        } else {
            let mut rows = element("tbody");
            for pod in &view.related {
                let target = ClusterRef {
                    kind: "Pod".into(),
                    namespace: pod.namespace.clone().unwrap_or_default(),
                    name: pod.name.clone(),
                };
                rows = rows.child(
                    element("tr")
                        .attr("data-related", pod.name.clone())
                        .child(
                            element("td").child(
                                link(&env.path(&format!("/cluster/Pod/view?{}", target.query())))
                                    .text(pod.name.clone()),
                            ),
                        )
                        .child(element("td").child(status_cell(&pod.status, pod.attention)))
                        .child(
                            element("td").child(span().class("gt-muted").text(pod.detail.clone())),
                        )
                        .child(
                            element("td").child(span().class("gt-muted").text(pod.usage.clone())),
                        ),
                );
            }
            div()
                .child(section(&env, "ui_cluster_related"))
                .child(
                    element("table")
                        .attr("id", "cluster-related")
                        .class("gt-table")
                        .child(table_head(
                            &env,
                            &[
                                "ui_col_name",
                                "ui_col_status",
                                "ui_col_detail",
                                "ui_col_usage",
                            ],
                        ))
                        .child(rows),
                )
                .into_view()
        };
        let shell = if object.kind == "Pod" && !object.namespace.is_empty() {
            terminal(&env, &state, &object, &container)
        } else {
            View::Empty
        };
        let log = if object.kind == "Pod" && !object.namespace.is_empty() {
            pod_log(
                &env,
                &state,
                &object,
                &view.containers,
                &container,
                &previous,
            )
        } else {
            View::Empty
        };

        div()
            .class("gt-page")
            .child(
                p().class("gt-crumb")
                    .child(link(&back).text(env.t.t(&format!("ui_kind_{}", object.kind)))),
            )
            .child(
                div()
                    .class("home-header")
                    .child(
                        h3().attr("id", "cluster-object-title")
                            .text(object.name.clone()),
                    )
                    .child(span().class("gt-muted").text(object.namespace.clone())),
            )
            .child(related)
            .child(section(&env, "ui_cluster_events"))
            .child(events_view)
            .child(log)
            .child(shell)
            .child(actions(&env, &state, &object, &replicas))
            .child(section(&env, "ui_cluster_yaml"))
            .child(yaml_view(
                &env,
                &state,
                &object,
                &view.yaml,
                &view.edit_yaml,
            ))
            .into_view()
    })
}

fn pod_log(
    env: &Env,
    state: &State,
    object: &ClusterRef,
    containers: &[String],
    container: &Field,
    previous: &Signal<bool>,
) -> View {
    let options: Vec<(String, String)> =
        containers.iter().map(|c| (c.clone(), c.clone())).collect();
    let fetch = {
        let (ask, object, container, previous) = (
            Rc::clone(&env.ask),
            object.clone(),
            container.clone(),
            previous.clone(),
        );
        let first = containers.first().cloned().unwrap_or_default();
        move || {
            let chosen = container.value_untracked();
            ask(Request::ClusterLog {
                namespace: object.namespace.clone(),
                pod: object.name.clone(),
                container: if chosen.is_empty() {
                    first.clone()
                } else {
                    chosen
                },
                previous: previous.get_untracked(),
            });
        }
    };
    let toggle = {
        let previous = previous.clone();
        move || previous.update(|p| *p = !*p)
    };
    let text = {
        let log = state.cluster_log.clone();
        View::dynamic(move || {
            log.get().map_or(View::Empty, |text| {
                element("pre")
                    .attr("id", "cluster-log")
                    .class("gt-log")
                    .text(text)
                    .into_view()
            })
        })
    };
    div()
        .child(section(env, "ui_cluster_logs"))
        .child(
            div()
                .class("gt-form gt-inline-form")
                .child(select_field(container, options, &[]))
                .child(
                    label()
                        .child(
                            input()
                                .attr("type", "checkbox")
                                .attr("id", "log-previous")
                                .on_click(toggle),
                        )
                        .child(span().text(env.t.t("ui_cluster_previous"))),
                )
                .child(
                    button()
                        .attr("type", "button")
                        .attr("id", "log-load")
                        .class("fui-btn")
                        .on_click(fetch)
                        .text(env.t.t("ui_cluster_load_log")),
                ),
        )
        .child(text)
        .into_view()
}

/// What may not be deleted from here (the service refuses these too; the button is not drawn).
const NOT_DELETED: [&str; 9] = [
    "Namespace",
    "CustomResourceDefinition",
    "PersistentVolume",
    "PersistentVolumeClaim",
    "StorageClass",
    "Node",
    "Event",
    "Secret",
    "ClusterRoleBinding",
];

/// Delete and scale, for whoever may.
fn actions(env: &Env, state: &State, object: &ClusterRef, replicas: &Field) -> View {
    if !state.info.with(|i| i.may("scale")) {
        return View::Empty;
    }
    let mut row = div()
        .class("gt-form gt-inline-form")
        .attr("id", "cluster-actions");
    if matches!(object.kind.as_str(), "Deployment" | "StatefulSet") {
        let ask = Rc::clone(&env.ask);
        let (target, field) = (object.clone(), replicas.clone());
        row = row
            .child(
                replicas
                    .input()
                    .class("gt-input")
                    .attr("type", "number")
                    .attr("min", "0")
                    .attr("max", "50"),
            )
            .child(
                button()
                    .attr("type", "button")
                    .attr("id", "scale")
                    .class("gt-btn")
                    .on_click(move || {
                        if let Ok(count) = field.value_untracked().trim().parse::<u32>() {
                            ask(Request::ClusterScale {
                                object: target.clone(),
                                replicas: count,
                            });
                        }
                    })
                    .text(env.t.t("ui_cluster_scale")),
            );
    }
    if !NOT_DELETED.contains(&object.kind.as_str()) {
        row = row.child(act_confirmed(
            env,
            state,
            "delete",
            &env.t.t("ui_action_delete"),
            "danger",
            env.tr(
                "ui_confirm_delete",
                &[
                    ("kind", object.kind.clone().into()),
                    ("name", object.name.clone().into()),
                ],
            ),
            Request::ClusterDelete(object.clone()),
        ));
    }
    row.into_view()
}

/// The YAML: read-only to look at, or a form to change it when the person may and the kind allows.
fn yaml_view(env: &Env, state: &State, object: &ClusterRef, shown: &str, editable: &str) -> View {
    if editable.is_empty() || !state.info.with(|i| i.may("deploy")) {
        return textarea()
            .class("gt-yaml")
            .attr("id", "cluster-yaml")
            .attr("readonly", "true")
            .attr("spellcheck", "false")
            .attr("rows", "24")
            .text(shown.to_string())
            .into_view();
    }
    let field = Field::new("cluster-yaml", editable);
    let ask = Rc::clone(&env.ask);
    let (target, text) = (object.clone(), field.clone());
    form()
        .attr("id", "cluster-edit")
        .class("gt-form")
        .on_submit(move || {
            ask(Request::ClusterEdit {
                object: target.clone(),
                yaml: text.value_untracked(),
            });
        })
        .child(
            field
                .bind(textarea())
                .attr("spellcheck", "false")
                .attr("rows", "24")
                .class("gt-yaml"),
        )
        .child(p().class("gt-desc").text(env.t.t("ui_cluster_edit_note")))
        .child(
            button()
                .attr("type", "submit")
                .attr("id", "cluster-apply")
                .class("gt-btn gt-btn-primary")
                .text(env.t.t("ui_edit_save")),
        )
        .into_view()
}

/// What the cluster serves, as links: a place that is up is a link, one that is not is greyed.
#[must_use]
pub fn services(env: &Env, state: &State) -> View {
    let (env, list, problem) = (env.clone(), state.facade.clone(), state.problem.clone());
    View::dynamic(move || {
        let Some(entries) = list.get() else {
            return if problem.get().is_some() {
                View::Empty
            } else {
                waiting(&env)
            };
        };
        if entries.is_empty() {
            return p()
                .attr("id", "no-services")
                .class("fui-empty")
                .text(env.t.t("ui_cluster_none"))
                .into_view();
        }
        let mut grid = div().class("gt-grid").attr("id", "services");
        for entry in entries {
            let card = if entry.available {
                a().attr("href", entry.url.clone()).attr("target", "_blank")
            } else {
                a().attr("aria-disabled", "true")
            };
            let state_word = env.t.t(if entry.available {
                "ui_cluster_up"
            } else {
                "ui_cluster_down"
            });
            grid = grid.child(
                card.class("gt-app")
                    .attr("data-service", entry.url.clone())
                    .child(
                        div()
                            .class("gt-app-head")
                            .child(strong().class("gt-app-name").text(entry.name.clone()))
                            .child(status_cell(&state_word, !entry.available)),
                    )
                    .child(p().class("gt-desc").text(entry.url.clone()))
                    .child(p().class("gt-muted").text(entry.namespace.clone())),
            );
        }
        grid.into_view()
    })
}

/// Who the person is to Gantry, and what they may do.
#[must_use]
pub fn profile(env: &Env, state: &State) -> View {
    let (env, info) = (env.clone(), state.info.clone());
    View::dynamic(move || {
        let info = info.get();
        let mut grants = div().class("gt-chips").attr("id", "profile-can");
        for action in &info.can {
            grants = grants.child(span().class("gt-chip").text(action.clone()));
        }
        div()
            .class("gt-page")
            .child(
                div()
                    .class("home-header")
                    .child(h3().text(env.t.t("ui_nav_profile"))),
            )
            .child(section(&env, "ui_profile_actor"))
            .child(p().attr("id", "profile-actor").text(info.actor.clone()))
            .child(section(&env, "ui_profile_can"))
            .child(grants)
            .child(section(&env, "ui_profile_version"))
            .child(p().class("gt-muted").text(info.version))
            .into_view()
    })
}

/// A shell in the pod: opened on request, shown as a terminal would, a line at a time.
#[allow(clippy::too_many_lines)]
fn terminal(env: &Env, state: &State, object: &ClusterRef, container: &Field) -> View {
    let (env, state, object, container) = (
        env.clone(),
        state.clone(),
        object.clone(),
        container.clone(),
    );
    let line = Field::new("exec-line", "");
    View::dynamic(move || {
        // Only where the operator turned shells on, and only for whoever may use them.
        let allowed = state.info.with(|i| i.may("deploy"))
            && state
                .exec_namespaces
                .with(|all| all.contains(&object.namespace));
        if !allowed {
            return View::Empty;
        }
        let Some(id) = state.exec_id.get() else {
            let ask = Rc::clone(&env.ask);
            let (target, chosen) = (object.clone(), container.clone());
            return div()
                .child(section(&env, "ui_cluster_shell"))
                .child(
                    button()
                        .attr("type", "button")
                        .attr("id", "exec-open")
                        .class("gt-btn")
                        .on_click(move || {
                            ask(Request::ExecOpen {
                                object: target.clone(),
                                container: chosen.value_untracked(),
                            });
                        })
                        .text(env.t.t("ui_cluster_shell_open")),
                )
                .into_view();
        };

        let screen = {
            let screen = state.exec_screen.clone();
            View::dynamic(move || {
                element("pre")
                    .attr("id", "exec-screen")
                    .class("gt-log")
                    .text(screen.get())
                    .into_view()
            })
        };
        let over = {
            let (env, over) = (env.clone(), state.exec_over.clone());
            View::dynamic(move || {
                over.get().map_or(View::Empty, |why| {
                    p().attr("id", "exec-over")
                        .class("gt-muted")
                        .text(if why.is_empty() {
                            env.t.t("ui_cluster_shell_ended")
                        } else {
                            format!("{} {why}", env.t.t("ui_cluster_shell_ended"))
                        })
                        .into_view()
                })
            })
        };
        let send = {
            let (ask, line, id, over) = (
                Rc::clone(&env.ask),
                line.clone(),
                id.clone(),
                state.exec_over.clone(),
            );
            move || {
                if over.get_untracked().is_some() {
                    return;
                }
                let mut data = line.value_untracked();
                data.push('\n');
                line.set("");
                ask(Request::ExecInput {
                    id: id.clone(),
                    data,
                });
            }
        };
        let close = {
            let (ask, id, state) = (Rc::clone(&env.ask), id, state.clone());
            move || {
                ask(Request::ExecClose(id.clone()));
                state.exec_id.set(None);
            }
        };
        div()
            .child(section(&env, "ui_cluster_shell"))
            .child(screen)
            .child(over)
            .child(
                form()
                    .attr("id", "exec-form")
                    .class("gt-form gt-inline-form")
                    .on_submit(send)
                    .child(
                        line.input()
                            .class("gt-input")
                            .attr("type", "text")
                            .attr("autocomplete", "off")
                            .attr("spellcheck", "false"),
                    )
                    .child(
                        button()
                            .attr("type", "submit")
                            .class("gt-btn gt-btn-primary")
                            .text(env.t.t("ui_cluster_shell_send")),
                    )
                    .child(
                        button()
                            .attr("type", "button")
                            .attr("id", "exec-close")
                            .class("gt-btn")
                            .on_click(close)
                            .text(env.t.t("ui_cluster_shell_close")),
                    ),
            )
            .into_view()
    })
}

/// Under "Cluster" in the sidebar: the overview, then every kind under its heading, the open one marked.
pub(super) fn subnav(env: &Env, state: &State, here: &str) -> View {
    let overview = env.path("/cluster");
    let here = here.trim_end_matches('/');
    let mut nav = div().class("gt-subnav").attr("id", "cluster-nav").child(
        link(&overview)
            .class(if here == overview {
                "gt-subnav-item gt-current"
            } else {
                "gt-subnav-item"
            })
            .attr("id", "sub-overview")
            .text(env.t.t("ui_cluster_overview")),
    );
    for (group, kinds) in kinds_by_group(&state.cluster_kinds.get()) {
        nav = nav.child(
            div()
                .class("gt-subnav-group")
                .text(env.t.t(&format!("ui_cluster_group_{group}"))),
        );
        for kind in kinds {
            let address = env.path(&format!("/cluster/{}", kind.kind));
            let open = here == address || here.starts_with(&format!("{address}/"));
            nav = nav.child(
                link(&address)
                    .class(if open {
                        "gt-subnav-item gt-current"
                    } else {
                        "gt-subnav-item"
                    })
                    .attr("id", format!("sub-{}", kind.kind))
                    .text(env.t.t(&format!("ui_kind_{}", kind.kind))),
            );
        }
    }
    nav.into_view()
}
