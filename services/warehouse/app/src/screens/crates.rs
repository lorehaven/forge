//! The crates registry: its crates with their versions, and one version's metadata with yank and unyank.

use quench_ui::prelude::*;
use warehouse_core::fmt::encode;
use warehouse_core::tree::active_crate_version;
use warehouse_core::{CrateDep, CrateVersion};

use super::{
    Env, dot, empty, meta, meta_text, metadata_title, nav, panel_title, split, status, waiting,
};
use crate::live::{Request, State};

#[must_use]
pub(crate) fn crates(env: &Env, state: &State) -> View {
    env.ask(Request::Crates);
    {
        let env = env.clone();
        create_effect(move || {
            let name = env.query("repo");
            if !name.is_empty() {
                untrack(|| env.ask(Request::CrateVersions(name)));
            }
        });
    }

    let left = {
        let (env, state) = (env.clone(), state.clone());
        View::dynamic(move || {
            let Some(names) = state.crates.get() else {
                return waiting(&env);
            };
            if names.is_empty() {
                return empty(env.t.t("ui_crates_empty"))
                    .attr("id", "no-crates")
                    .into_view();
            }
            let selected = env.query("repo");
            let wanted = env.query("tag");
            let versions = versions_of(&state, &selected);
            let active =
                active_crate_version(&versions, Some(wanted.as_str()).filter(|w| !w.is_empty()))
                    .map(|i| versions[i].vers.clone());
            let mut list = ul().class("wh-tree");
            for name in &names {
                let link_to = nav(
                    &env,
                    &format!("{}?repo={}", env.path("/crates/catalog"), encode(name)),
                )
                .class(if *name == selected {
                    "wh-link active"
                } else {
                    "wh-link"
                })
                .attr("data-crate", name.clone())
                .text(name.clone());
                if *name != selected {
                    list = list.child(li().child(div().class("wh-folder").child(link_to)));
                    continue;
                }
                let mut tags = ul().class("wh-tags");
                if versions.is_empty() {
                    tags = tags.child(
                        li().child(
                            span()
                                .class("wh-tag wh-faint")
                                .text(env.t.t("ui_empty_no_versions")),
                        ),
                    );
                }
                for version in versions.iter().rev() {
                    tags = tags.child(
                        li().child(
                            nav(
                                &env,
                                &format!(
                                    "{}?repo={}&tag={}",
                                    env.path("/crates/catalog"),
                                    encode(name),
                                    encode(&version.vers)
                                ),
                            )
                            .class(if active.as_deref() == Some(version.vers.as_str()) {
                                "wh-tag active"
                            } else {
                                "wh-tag"
                            })
                            .attr("data-version", version.vers.clone())
                            .child(dot(version.yanked))
                            .child(span().text(version.vers.clone())),
                        ),
                    );
                }
                list = list.child(
                    li().child(
                        element("details")
                            .attr("data-path", name.clone())
                            .attr("open", "open")
                            .child(element("summary").class("wh-folder").child(link_to))
                            .child(tags),
                    ),
                );
            }
            div().class("wh-scroll").child(list).into_view()
        })
    };

    let right = {
        let (env, state) = (env.clone(), state.clone());
        View::dynamic(move || {
            let selected = env.query("repo");
            let wanted = env.query("tag");
            let versions = versions_of(&state, &selected);
            let record =
                active_crate_version(&versions, Some(wanted.as_str()).filter(|w| !w.is_empty()))
                    .map(|i| versions[i].clone());
            let body = record.as_ref().map_or_else(
                || empty(env.t.t("ui_empty_select_version")).into_view(),
                |record| details(&env, &selected, record).into_view(),
            );
            div()
                .class("wh-detail")
                .child(metadata_title(
                    &env,
                    record.as_ref().map(|r| r.vers.as_str()),
                ))
                .child(body)
                .into_view()
        })
    };

    split(
        div()
            .class("wh-pane")
            .child(panel_title(env.t.t("ui_crates")))
            .child(left),
        div().class("wh-pane").child(right),
    )
}

/// The versions loaded for `name`, oldest first (none when another crate's are the ones loaded).
fn versions_of(state: &State, name: &str) -> Vec<CrateVersion> {
    state
        .versions
        .with(|v| {
            v.as_ref()
                .filter(|(n, _)| n == name)
                .map(|(_, v)| v.clone())
        })
        .unwrap_or_default()
}

fn details(env: &Env, name: &str, record: &CrateVersion) -> Element {
    let mut list = div()
        .class("wh-meta-list")
        .child(meta_text(env.t.t("ui_meta_version"), &record.vers))
        .child(meta(env.t.t("ui_meta_status"), status(env, record.yanked)))
        .child(meta_text(env.t.t("ui_meta_checksum"), &record.cksum));
    if let Some(rust) = &record.rust_version {
        list = list.child(meta_text(env.t.t("ui_meta_rust_version"), rust));
    }
    if let Some(links) = &record.links {
        list = list.child(meta_text(env.t.t("ui_meta_links"), links));
    }
    if !record.features.is_empty() {
        list = list.child(meta_text(
            env.t.t("ui_meta_features"),
            &record.features.join(", "),
        ));
    }
    if !record.deps.is_empty() {
        list = list.child(dependencies(env, &record.deps));
    }

    let (label, id) = if record.yanked {
        (env.t.t("ui_unyank_version"), "unyank")
    } else {
        (env.t.t("ui_yank_version"), "yank")
    };
    let ask = {
        let (env, name, version, yanked) = (
            env.clone(),
            name.to_string(),
            record.vers.clone(),
            !record.yanked,
        );
        move || {
            env.pin("tag", &version);
            env.ask(Request::SetCrateYank {
                name: name.clone(),
                version: version.clone(),
                yanked,
            });
        }
    };
    list.child(
        div().class("wh-actions-row").child(
            button()
                .attr("type", "button")
                .attr("id", id)
                .class(if record.yanked {
                    "wh-btn wh-warn-btn"
                } else {
                    "wh-btn wh-danger"
                })
                .on_click(ask)
                .text(label),
        ),
    )
}

/// The dependencies in groups: normal, build, dev.
fn dependencies(env: &Env, deps: &[CrateDep]) -> Element {
    let mut section = div().class("wh-deps");
    for (kinds, key) in [
        (&["normal"][..], "ui_deps_normal"),
        (&["build"][..], "ui_deps_build"),
        (&["dev"][..], "ui_deps_dev"),
    ] {
        let group: Vec<&CrateDep> = deps
            .iter()
            .filter(|d| {
                let known = matches!(d.kind.as_str(), "dev" | "build");
                kinds.contains(&d.kind.as_str()) || (kinds == ["normal"] && !known)
            })
            .collect();
        if group.is_empty() {
            continue;
        }
        let mut rows = div()
            .class("wh-deps-group")
            .child(div().class("wh-deps-label").text(env.t.t(key)));
        for dep in group {
            rows = rows.child(div().class("wh-dep wh-mono").text(dep.line()));
        }
        section = section.child(rows);
    }
    meta(env.t.t("ui_meta_deps"), section)
}
