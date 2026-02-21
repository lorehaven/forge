use crate::routers::docker::registry::storage::{
    TagListError, TagMetadata, list_repositories, list_tag_metadata_for_repository,
};
use crate::routers::ui::PageQuery;
use crate::routers::ui::common::{UiPageKind, is_ui_authenticated, render_page, ui_login_redirect};
use crate::shared::jwt::JwtConfig;
use actix_web::{HttpRequest, HttpResponse, Responder, get, web};
use quench::prelude::*;
use std::collections::BTreeMap;

#[derive(Default)]
struct RepoTreeNode {
    children: BTreeMap<String, RepoTreeNode>,
    full_repo: Option<String>,
}

#[get("/docker/catalog")]
pub(super) async fn docker_catalog(
    req: HttpRequest,
    query: web::Query<PageQuery>,
    config: web::Data<JwtConfig>,
) -> impl Responder {
    if !is_ui_authenticated(&req, &config) {
        return ui_login_redirect();
    }
    render_catalog_page(query.repo.clone(), query.tag.clone())
}

#[get("/docker/catalog/")]
pub(super) async fn docker_catalog_slash(
    req: HttpRequest,
    query: web::Query<PageQuery>,
    config: web::Data<JwtConfig>,
) -> impl Responder {
    if !is_ui_authenticated(&req, &config) {
        return ui_login_redirect();
    }
    render_catalog_page(query.repo.clone(), query.tag.clone())
}

fn render_catalog_page(
    selected_repo: Option<String>,
    selected_tag: Option<String>,
) -> HttpResponse {
    let repositories = list_repositories();
    let tree = build_repo_tree(&repositories);

    let repo = selected_repo
        .as_ref()
        .filter(|r| repositories.iter().any(|x| x == *r))
        .cloned();

    let tags_meta = match repo.as_deref() {
        Some(repo) => match list_tag_metadata_for_repository(repo) {
            Ok(v) => v,
            Err(TagListError::InvalidName) | Err(TagListError::NotFound) => Vec::new(),
        },
        None => Vec::new(),
    };

    let active_tag = selected_tag
        .as_ref()
        .filter(|tag| tags_meta.iter().any(|meta| &meta.tag == *tag))
        .cloned()
        .or_else(|| tags_meta.first().map(|m| m.tag.clone()));

    let selected_meta = active_tag
        .as_ref()
        .and_then(|tag| tags_meta.iter().find(|m| &m.tag == tag));

    let left = div()
        .class("split-left")
        .child(
            div()
                .class("panel-title")
                .attr("data-i18n", "ui_repositories"),
        )
        .child(
            div()
                .class("tree-scroll")
                .child(render_repo_tree(&tree, repo.as_deref())),
        );

    let right = div()
        .class("split-right")
        .child(div().class("right-top").child(render_tags_panel(
            repo.as_deref(),
            &tags_meta,
            active_tag.as_deref(),
        )))
        .child(
            div()
                .class("right-bottom")
                .child(render_metadata_panel(repo.as_deref(), selected_meta)),
        );

    render_page(
        HttpResponse::Ok(),
        content()
            .class("container-fluid py-4")
            .child(div().class("split-view").child(left).child(right)),
        UiPageKind::Docker,
    )
}

fn render_tags_panel(
    repo: Option<&str>,
    tags_meta: &[TagMetadata],
    active_tag: Option<&str>,
) -> Element {
    let title = match repo {
        Some(r) => div()
            .class("panel-title")
            .child(span().attr("data-i18n", "ui_tags_for"))
            .child(span().text(&format!(" {r}"))),
        None => div().class("panel-title").attr("data-i18n", "ui_tags"),
    };

    let mut body = div().class("body");
    if repo.is_none() {
        body = body.child(
            div()
                .class("empty")
                .attr("data-i18n", "ui_empty_select_repo"),
        );
    } else if tags_meta.is_empty() {
        body = body.child(div().class("empty").attr("data-i18n", "ui_empty_no_tags"));
    } else {
        for meta in tags_meta {
            let Some(repo_name) = repo else { break };
            let link = format!("/ui/docker/catalog?repo={repo_name}&tag={}", meta.tag);
            let row_class = if Some(meta.tag.as_str()) == active_tag {
                "row active"
            } else {
                "row"
            };

            body = body.child(
                div()
                    .class(row_class)
                    .child(
                        div()
                            .class("cell")
                            .child(a().attr("href", &link).class("tag-link").text(&meta.tag)),
                    )
                    .child(div().class("cell mono").text(&short_digest(&meta.digest)))
                    .child(
                        div()
                            .class("cell mono")
                            .text(meta.media_type.as_deref().unwrap_or("-")),
                    ),
            );
        }
    }

    div()
        .class("panel table tags-grid")
        .child(title)
        .child(
            div()
                .class("header")
                .child(div().class("cell").attr("data-i18n", "ui_col_tag"))
                .child(div().class("cell").attr("data-i18n", "ui_col_digest"))
                .child(div().class("cell").attr("data-i18n", "ui_col_media_type")),
        )
        .child(body)
}

fn render_metadata_panel(repo: Option<&str>, selected_meta: Option<&TagMetadata>) -> Element {
    let title = match (repo, selected_meta) {
        (Some(_), Some(meta)) => div()
            .class("panel-title")
            .child(span().attr("data-i18n", "ui_metadata_for"))
            .child(span().text(&format!(" {}", meta.tag))),
        _ => div().class("panel-title").attr("data-i18n", "ui_metadata"),
    };

    let body = match selected_meta {
        Some(meta) => div()
            .class("meta-list")
            .child(meta_row("ui_meta_tag", &meta.tag))
            .child(meta_row("ui_meta_digest", &meta.digest))
            .child(meta_row(
                "ui_meta_media_type",
                meta.media_type.as_deref().unwrap_or("unknown"),
            ))
            .child(meta_row(
                "ui_meta_manifest_size",
                &meta
                    .size_bytes
                    .map(|v| format!("{v} bytes"))
                    .unwrap_or_else(|| "unknown".to_string()),
            )),
        None => div()
            .class("empty")
            .attr("data-i18n", "ui_empty_select_tag"),
    };

    div().class("panel").child(title).child(body)
}

fn meta_row(label_key: &str, value: &str) -> Element {
    div()
        .class("meta-row")
        .child(div().class("meta-label").attr("data-i18n", label_key))
        .child(div().class("meta-value mono").text(value))
}

fn short_digest(digest: &str) -> String {
    if digest.len() <= 20 {
        return digest.to_string();
    }

    format!("{}...{}", &digest[..12], &digest[digest.len() - 8..])
}

fn build_repo_tree(repositories: &[String]) -> RepoTreeNode {
    let mut root = RepoTreeNode::default();

    for repo in repositories {
        let mut node = &mut root;
        for segment in repo.split('/') {
            node = node.children.entry(segment.to_string()).or_default();
        }
        node.full_repo = Some(repo.clone());
    }

    root
}

fn render_repo_tree(root: &RepoTreeNode, selected_repo: Option<&str>) -> Element {
    let mut tree = ul().class("repo-tree");
    for (name, child) in &root.children {
        tree = tree.child(render_repo_node(name, child, selected_repo));
    }
    tree
}

fn render_repo_node(name: &str, node: &RepoTreeNode, selected_repo: Option<&str>) -> Element {
    let mut item = li();

    if node.children.is_empty() {
        if let Some(repo) = node.full_repo.as_deref() {
            let class_name = if Some(repo) == selected_repo {
                "repo-link active"
            } else {
                "repo-link"
            };
            item = item.child(
                a().attr("href", &format!("/ui/docker/catalog?repo={repo}"))
                    .class(class_name)
                    .text(name),
            );
        } else {
            item = item.child(span().text(name));
        }
        return item;
    }

    let mut details = element("details");
    if selected_repo.is_some_and(|selected| node_has_selected(node, selected)) {
        details = details.attr("open", "open");
    }

    let mut children = ul();
    for (child_name, child_node) in &node.children {
        children = children.child(render_repo_node(child_name, child_node, selected_repo));
    }

    item.child(
        details
            .child(element("summary").class("tree-folder").text(name))
            .child(children),
    )
}

fn node_has_selected(node: &RepoTreeNode, selected: &str) -> bool {
    if node.full_repo.as_deref() == Some(selected) {
        return true;
    }

    node.children
        .values()
        .any(|child| node_has_selected(child, selected))
}
