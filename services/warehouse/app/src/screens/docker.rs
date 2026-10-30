//! The docker registry: a tree of repositories with their tags, and one tag's metadata.

use quench_ui::i18n::Value;
use quench_ui::prelude::*;
use warehouse_core::DockerTag;
use warehouse_core::fmt::encode;
use warehouse_core::tree::{RepoNode, repo_tree};

use super::{
    Env, confirm, empty, meta, meta_text, metadata_title, nav, panel_title, split, waiting,
};
use crate::live::{Request, State};

#[must_use]
pub(crate) fn docker(env: &Env, state: &State) -> View {
    env.ask(Request::DockerRepos);
    // The tags of whichever repository the address names.
    {
        let env = env.clone();
        create_effect(move || {
            let repo = env.query("repo");
            if !repo.is_empty() {
                untrack(|| env.ask(Request::DockerTags(repo)));
            }
        });
    }

    let left = {
        let (env, state) = (env.clone(), state.clone());
        View::dynamic(move || {
            let Some(repos) = state.repos.get() else {
                return waiting(&env);
            };
            if repos.is_empty() {
                return empty(env.t.t("ui_docker_empty"))
                    .attr("id", "no-repos")
                    .into_view();
            }
            let selected = env.query("repo");
            let tag = env.query("tag");
            let tags = tags_of(&state, &selected);
            let tree = repo_tree(&repos);
            let mut list = ul().class("wh-tree");
            for (name, node) in &tree.children {
                list = list.child(node_view(&env, name, node, "", &selected, &tags, &tag));
            }
            div().class("wh-scroll").child(list).into_view()
        })
    };

    let right = {
        let (env, state) = (env.clone(), state.clone());
        View::dynamic(move || {
            let selected = env.query("repo");
            let tag = env.query("tag");
            let tags = tags_of(&state, &selected);
            let meta_of = tags.iter().find(|t| t.tag == tag);
            let body = meta_of.map_or_else(
                || empty(env.t.t("ui_empty_select_tag")).into_view(),
                |meta_of| details(&env, &state, &selected, meta_of).into_view(),
            );
            div()
                .class("wh-detail")
                .child(metadata_title(&env, meta_of.map(|t| t.tag.as_str())))
                .child(body)
                .into_view()
        })
    };

    split(
        div()
            .class("wh-pane")
            .child(panel_title(env.t.t("ui_repositories")))
            .child(left),
        div().class("wh-pane").child(right),
    )
}

/// The tags loaded for `repo` (none when another repository's are the ones loaded).
fn tags_of(state: &State, repo: &str) -> Vec<DockerTag> {
    state
        .tags
        .with(|t| {
            t.as_ref()
                .filter(|(r, _)| r == repo)
                .map(|(_, tags)| tags.clone())
        })
        .unwrap_or_default()
}

fn href(env: &Env, repo: &str, tag: Option<&str>) -> String {
    let mut href = format!("{}?repo={}", env.path("/docker/catalog"), encode(repo));
    if let Some(tag) = tag {
        href = format!("{href}&tag={}", encode(tag));
    }
    href
}

fn node_view(
    env: &Env,
    name: &str,
    node: &RepoNode,
    parent: &str,
    selected: &str,
    tags: &[DockerTag],
    active_tag: &str,
) -> Element {
    let full = if parent.is_empty() {
        name.to_string()
    } else {
        format!("{parent}/{name}")
    };
    let is_selected = node.repo.as_deref() == Some(selected) && !selected.is_empty();
    let on_path = !selected.is_empty() && node.holds(selected);
    let repo_link = |repo: &str, active: bool| {
        nav(env, &href(env, repo, None))
            .class(if active { "wh-link active" } else { "wh-link" })
            .attr("data-repo", repo.to_string())
            .text(name.to_string())
    };

    if node.children.is_empty() && !is_selected {
        return li().child(match &node.repo {
            Some(repo) => div().class("wh-folder").child(repo_link(repo, false)),
            None => div()
                .class("wh-folder")
                .child(span().text(name.to_string())),
        });
    }

    let mut details = element("details").attr("data-path", full.clone());
    if on_path {
        details = details.attr("open", "open");
    }
    let summary = element("summary")
        .class("wh-folder")
        .child(match &node.repo {
            Some(repo) => repo_link(repo, is_selected).into_view(),
            None => span().text(name.to_string()).into_view(),
        });
    details = details.child(summary);

    if !node.children.is_empty() {
        let mut children = ul().class("wh-tree");
        for (child_name, child) in &node.children {
            children = children.child(node_view(
                env, child_name, child, &full, selected, tags, active_tag,
            ));
        }
        details = details.child(children);
    }
    if is_selected && !tags.is_empty() {
        let mut list = ul().class("wh-tags");
        for meta_of in tags {
            let active = meta_of.tag == active_tag;
            list = list.child(
                li().child(
                    nav(env, &href(env, selected, Some(&meta_of.tag)))
                        .class(if active { "wh-tag active" } else { "wh-tag" })
                        .attr("data-tag", meta_of.tag.clone())
                        .child(span().text(meta_of.tag.clone())),
                ),
            );
        }
        details = details.child(list);
    }
    li().child(details)
}

fn details(env: &Env, state: &State, repo: &str, tag: &DockerTag) -> Element {
    let unknown = || span().text(env.t.t("ui_meta_unknown"));
    let media = tag.media_type.as_deref().map_or_else(
        || meta(env.t.t("ui_meta_media_type"), unknown()),
        |m| meta_text(env.t.t("ui_meta_media_type"), m),
    );
    let size = tag.size_bytes.map_or_else(
        || meta(env.t.t("ui_meta_manifest_size"), unknown()),
        |size| {
            meta(
                env.t.t("ui_meta_manifest_size"),
                span().text(
                    env.t
                        .tr("ui_meta_bytes", &[("size", Value::Text(size.to_string()))]),
                ),
            )
        },
    );
    let delete = {
        let (state, repo, tag) = (state.clone(), repo.to_string(), tag.clone());
        move || {
            confirm(
                &state,
                "ui_modal_delete_title",
                "ui_docker_delete_confirm_text",
                format!("{repo}:{}", tag.tag),
                Request::DeleteImage {
                    repository: repo.clone(),
                    digest: tag.digest.clone(),
                },
            );
        }
    };
    div()
        .class("wh-meta-list")
        .child(meta_text(env.t.t("ui_meta_tag"), &tag.tag))
        .child(meta_text(env.t.t("ui_meta_digest"), &tag.digest))
        .child(media)
        .child(size)
        .child(
            div().class("wh-actions-row").child(
                button()
                    .attr("type", "button")
                    .attr("id", "delete-image")
                    .class("wh-btn wh-danger")
                    .on_click(delete)
                    .text(env.t.t("ui_delete_image")),
            ),
        )
}
