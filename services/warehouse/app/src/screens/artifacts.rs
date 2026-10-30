//! The artifact registry: programs, their platforms and builds, and one build's metadata with yank and unyank.

use quench_ui::prelude::*;
use std::collections::BTreeMap;
use warehouse_core::fmt::encode;

use warehouse_core::ArtifactEntry;
use warehouse_core::tree::{artifact_tree, pick_artifact};

use super::{
    Env, dot, empty, meta, meta_text, metadata_title, nav, panel_title, split, status, waiting,
};
use crate::live::{Request, State};

#[must_use]
pub(crate) fn artifacts(env: &Env, state: &State) -> View {
    env.ask(Request::Artifacts);

    // The tree of the builds, drawn again when they change; what is open follows the address.
    let tree = {
        let state = state.clone();
        create_memo(move || {
            state
                .artifacts
                .with(|a| a.as_ref().map(|a| artifact_tree(a)))
        })
    };

    let left = {
        let (env, tree) = (env.clone(), tree.clone());
        View::dynamic(move || {
            let Some(tree) = tree.get() else {
                return waiting(&env);
            };
            if tree.is_empty() {
                return empty(env.t.t("ui_artifact_empty"))
                    .attr("id", "no-artifacts")
                    .into_view();
            }
            let code = env.query("tag").parse::<i64>().ok();
            let pick = pick_artifact(
                &tree,
                Some(env.query("repo").as_str()).filter(|s| !s.is_empty()),
                Some(env.query("platform").as_str()).filter(|s| !s.is_empty()),
                code,
            );
            let mut list = ul().class("wh-tree");
            for (program, platforms) in &tree {
                list = list.child(program_node(
                    &env,
                    program,
                    platforms,
                    pick.program,
                    pick.platform,
                    pick.selected,
                ));
            }
            div().class("wh-scroll").child(list).into_view()
        })
    };

    let right = {
        let (env, state, tree) = (env.clone(), state.clone(), tree);
        View::dynamic(move || {
            let can_manage = state
                .overview
                .with(|o| o.as_ref().is_some_and(|o| o.can_manage));
            let found = tree.with(|tree| {
                tree.as_ref().and_then(|tree| {
                    let code = env.query("tag").parse::<i64>().ok();
                    pick_artifact(
                        tree,
                        Some(env.query("repo").as_str()).filter(|s| !s.is_empty()),
                        Some(env.query("platform").as_str()).filter(|s| !s.is_empty()),
                        code,
                    )
                    .selected
                    .cloned()
                })
            });
            let body = found.as_ref().map_or_else(
                || empty(env.t.t("ui_artifact_empty_select_version")).into_view(),
                |v| details(&env, v, can_manage).into_view(),
            );
            let which = found
                .as_ref()
                .map(|v| format!("{} ({})", v.version_name, v.version_code));
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
            .child(panel_title(env.t.t("ui_artifact_programs")))
            .child(left),
        div().class("wh-pane").child(right),
    )
}

fn program_node(
    env: &Env,
    program: &str,
    platforms: &BTreeMap<String, Vec<ArtifactEntry>>,
    selected_program: Option<&str>,
    selected_platform: Option<&str>,
    selected: Option<&ArtifactEntry>,
) -> Element {
    let at = format!(
        "{}?repo={}",
        env.path("/artifacts/catalog"),
        encode(program)
    );
    let is_selected = selected_program == Some(program);
    let program_link = |active: bool| {
        nav(env, &at)
            .class(if active { "wh-link active" } else { "wh-link" })
            .attr("data-program", program.to_string())
            .text(program.to_string())
    };
    if !is_selected {
        return li().child(div().class("wh-folder").child(program_link(false)));
    }

    let mut details = element("details")
        .attr("data-path", program.to_string())
        .attr("open", "open")
        .child(
            element("summary")
                .class("wh-folder")
                .child(program_link(true)),
        );
    for (platform, versions) in platforms {
        let open = selected_platform == Some(platform.as_str());
        let platform_link = nav(env, &format!("{at}&platform={}", encode(platform)))
            .class(if open { "wh-link active" } else { "wh-link" })
            .attr("data-platform", platform.clone())
            .text(platform.clone());
        let mut node = element("details")
            .attr("data-path", platform.clone())
            .child(element("summary").class("wh-folder").child(platform_link));
        if open {
            node = node.attr("open", "open");
            let open_code = selected.map(|v| v.version_code);
            let mut list = ul().class("wh-tags");
            for version in versions {
                let active = Some(version.version_code) == open_code;
                list = list.child(
                    li().child(
                        nav(
                            env,
                            &format!(
                                "{at}&platform={}&tag={}",
                                encode(platform),
                                version.version_code
                            ),
                        )
                        .class(if active { "wh-tag active" } else { "wh-tag" })
                        .attr("data-version", version.version_code.to_string())
                        .child(dot(version.yanked))
                        .child(span().text(format!(
                            "{} ({})",
                            version.version_name, version.version_code
                        ))),
                    ),
                );
            }
            node = node.child(list);
        }
        details = details.child(node);
    }
    li().child(details)
}

/// Everything known of one published build, and (to those who may) the way to yank it.
fn details(env: &Env, v: &ArtifactEntry, can_manage: bool) -> Element {
    let label = |key: &str| env.t.t(key);
    let mut list = div()
        .class("wh-meta-list")
        .child(meta_text(label("ui_artifact_meta_program"), &v.program))
        .child(meta_text(label("ui_artifact_meta_platform"), &v.platform))
        .child(meta_text(label("ui_artifact_meta_format"), &v.format))
        .child(meta_text(
            label("ui_artifact_meta_version_name"),
            &v.version_name,
        ))
        .child(meta_text(
            label("ui_artifact_meta_version_code"),
            &v.version_code.to_string(),
        ))
        .child(meta(label("ui_meta_status"), status(env, v.yanked)));
    if let Some(arch) = &v.arch {
        list = list.child(meta_text(label("ui_artifact_meta_arch"), arch));
    }
    if let Some(name) = &v.label {
        list = list.child(meta_text(label("ui_artifact_meta_label"), name));
    }
    if let Some(sdk) = v.min_sdk_version {
        list = list.child(meta_text(
            label("ui_artifact_meta_min_sdk"),
            &sdk.to_string(),
        ));
    }
    if let Some(sdk) = v.target_sdk_version {
        list = list.child(meta_text(
            label("ui_artifact_meta_target_sdk"),
            &sdk.to_string(),
        ));
    }
    list = list
        .child(meta_text(label("ui_artifact_meta_filename"), &v.filename))
        .child(meta_text(
            label("ui_artifact_meta_size"),
            &format!("{} bytes", v.size_bytes),
        ))
        .child(meta_text(label("ui_meta_checksum"), &v.sha256))
        .child(meta_text(
            label("ui_artifact_meta_uploaded_by"),
            &v.uploaded_by,
        ));
    if !v.permissions.is_empty() {
        list = list.child(meta_text(
            label("ui_artifact_meta_permissions"),
            &v.permissions.join(", "),
        ));
    }
    if !can_manage {
        return list;
    }
    let ask = {
        let (env, v) = (env.clone(), v.clone());
        move || {
            env.ask(Request::SetArtifactYank {
                program: v.program.clone(),
                platform: v.platform.clone(),
                version_code: v.version_code,
                yanked: !v.yanked,
            });
        }
    };
    let (text, id) = if v.yanked {
        (env.t.t("ui_artifact_unyank"), "unyank")
    } else {
        (env.t.t("ui_artifact_yank"), "yank")
    };
    list.child(
        div().class("wh-actions-row").child(
            button()
                .attr("type", "button")
                .attr("id", id)
                .class(if v.yanked {
                    "wh-btn wh-warn-btn"
                } else {
                    "wh-btn wh-danger"
                })
                .on_click(ask)
                .text(text),
        ),
    )
}
