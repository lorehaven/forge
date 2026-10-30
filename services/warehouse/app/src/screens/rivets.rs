//! The rivet package registry: packages with their versions, and one version's metadata with yank and unyank.

use quench_ui::prelude::*;
use warehouse_core::RivetEntry;
use warehouse_core::fmt::encode;
use warehouse_core::tree::{pick_rivet, rivet_tree};

use super::{
    Env, dot, empty, meta, meta_text, metadata_title, nav, panel_title, split, status, waiting,
};
use crate::live::{Request, State};

#[must_use]
pub(crate) fn rivets(env: &Env, state: &State) -> View {
    env.ask(Request::Rivets);

    let tree = {
        let state = state.clone();
        create_memo(move || state.rivets.with(|r| r.as_ref().map(|r| rivet_tree(r))))
    };

    let left = {
        let (env, tree) = (env.clone(), tree.clone());
        View::dynamic(move || {
            let Some(tree) = tree.get() else {
                return waiting(&env);
            };
            if tree.is_empty() {
                return empty(env.t.t("ui_rivet_empty"))
                    .attr("id", "no-rivets")
                    .into_view();
            }
            let selected = env.query("repo");
            let wanted = env.query("tag");
            let mut list = ul().class("wh-tree");
            for (name, versions) in &tree {
                let at = format!("{}?repo={}", env.path("/rivets/catalog"), encode(name));
                let is_selected = *name == selected;
                let package = nav(&env, &at)
                    .class(if is_selected {
                        "wh-link active"
                    } else {
                        "wh-link"
                    })
                    .attr("data-package", name.clone())
                    .text(name.clone());
                if !is_selected {
                    list = list.child(li().child(div().class("wh-folder").child(package)));
                    continue;
                }
                let open = pick_rivet(versions, Some(wanted.as_str()).filter(|w| !w.is_empty()))
                    .map(|p| p.version.clone());
                let mut tags = ul().class("wh-tags");
                for entry in versions {
                    tags = tags.child(
                        li().child(
                            nav(&env, &format!("{at}&tag={}", encode(&entry.version)))
                                .class(if open.as_deref() == Some(entry.version.as_str()) {
                                    "wh-tag active"
                                } else {
                                    "wh-tag"
                                })
                                .attr("data-version", entry.version.clone())
                                .child(dot(entry.yanked))
                                .child(span().text(entry.version.clone())),
                        ),
                    );
                }
                list = list.child(
                    li().child(
                        element("details")
                            .attr("data-path", name.clone())
                            .attr("open", "open")
                            .child(element("summary").class("wh-folder").child(package))
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
            let can_manage = state
                .overview
                .with(|o| o.as_ref().is_some_and(|o| o.can_manage));
            let selected = env.query("repo");
            let wanted = env.query("tag");
            let found = tree.with(|tree| {
                tree.as_ref().and_then(|tree| {
                    tree.get(&selected).and_then(|versions| {
                        pick_rivet(versions, Some(wanted.as_str()).filter(|w| !w.is_empty()))
                            .cloned()
                    })
                })
            });
            let body = found.as_ref().map_or_else(
                || empty(env.t.t("ui_rivet_empty_select_version")).into_view(),
                |p| details(&env, p, can_manage).into_view(),
            );
            let which = found.as_ref().map(|p| format!("{} {}", p.name, p.version));
            div()
                .class("wh-detail")
                .child(metadata_title(&env, which.as_deref()))
                .child(body)
                .into_view()
        })
    };

    split(
        div()
            .class("wh-pane")
            .child(panel_title(env.t.t("ui_rivet_packages")))
            .child(left),
        div().class("wh-pane").child(right),
    )
}

/// Everything known of one published package version, and (to those who may) the way to yank it.
fn details(env: &Env, p: &RivetEntry, can_manage: bool) -> Element {
    let label = |key: &str| env.t.t(key);
    let mut list = div()
        .class("wh-meta-list")
        .child(meta_text(label("ui_rivet_meta_name"), &p.name))
        .child(meta_text(label("ui_rivet_meta_version"), &p.version))
        .child(meta(label("ui_meta_status"), status(env, p.yanked)));
    if let Some(description) = &p.description {
        list = list.child(meta_text(label("ui_rivet_meta_description"), description));
    }
    if let Some(namespace) = &p.namespace {
        list = list.child(meta_text(label("ui_rivet_meta_namespace"), namespace));
    }
    if let Some(riveter) = &p.requires_riveter {
        list = list.child(meta_text(label("ui_rivet_meta_requires_riveter"), riveter));
    }
    if !p.requires_packages.is_empty() {
        list = list.child(meta_text(
            label("ui_rivet_meta_requires_packages"),
            &p.requires_packages.join(", "),
        ));
    }
    for pair in &p.meta {
        list = list.child(meta_text(pair.key.clone(), &pair.value));
    }
    list = list
        .child(meta_text(
            label("ui_artifact_meta_size"),
            &format!("{} bytes", p.size_bytes),
        ))
        .child(meta_text(label("ui_meta_checksum"), &p.sha256))
        .child(meta_text(
            label("ui_rivet_meta_uploaded_by"),
            &p.uploaded_by,
        ))
        .child(meta_text(label("ui_rivet_meta_published"), &p.published))
        .child(meta_text(
            label("ui_rivet_meta_install"),
            &format!("riveter install {}@{}", p.name, p.version),
        ));
    if !can_manage {
        return list;
    }
    let ask = {
        let (env, p) = (env.clone(), p.clone());
        move || {
            env.pin("tag", &p.version);
            env.ask(Request::SetRivetYank {
                name: p.name.clone(),
                version: p.version.clone(),
                yanked: !p.yanked,
            });
        }
    };
    let (text, id) = if p.yanked {
        (env.t.t("ui_rivet_unyank"), "unyank")
    } else {
        (env.t.t("ui_rivet_yank"), "yank")
    };
    list.child(
        div().class("wh-actions-row").child(
            button()
                .attr("type", "button")
                .attr("id", id)
                .class(if p.yanked {
                    "wh-btn wh-warn-btn"
                } else {
                    "wh-btn wh-danger"
                })
                .on_click(ask)
                .text(text),
        ),
    )
}
