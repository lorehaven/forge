use crate::routers::docker::registry::storage::{
    TagListError, TagMetadata, list_repositories, list_tag_metadata_for_repository,
};
use crate::routers::docker::{manifest_path, repository_path, validate_digest};
use crate::routers::ui::PageQuery;
use crate::routers::ui::common::{
    UiPageKind, is_ui_authenticated, render_page, ui_login_redirect, ui_path,
};
use actix_web::{HttpRequest, HttpResponse, Responder, get, post, web};
use quench_srv::prelude::JwtConfig;
use quench_srv::prelude::with_base_path;
use quench_web::prelude::*;
use std::collections::BTreeMap;

#[derive(serde::Deserialize)]
pub(in crate::routers::ui::pages) struct DeleteImageForm {
    repository: String,
    digest: String,
}

#[derive(serde::Deserialize)]
pub(in crate::routers::ui::pages) struct DeleteImageModalQuery {
    repository: String,
    tag: Option<String>,
    digest: String,
}

#[derive(Default)]
struct RepoTreeNode {
    children: BTreeMap<String, RepoTreeNode>,
    full_repo: Option<String>,
}

#[get("/docker/catalog")]
pub(in crate::routers::ui::pages) async fn docker_catalog(
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
pub(in crate::routers::ui::pages) async fn docker_catalog_slash(
    req: HttpRequest,
    query: web::Query<PageQuery>,
    config: web::Data<JwtConfig>,
) -> impl Responder {
    if !is_ui_authenticated(&req, &config) {
        return ui_login_redirect();
    }
    render_catalog_page(query.repo.clone(), query.tag.clone())
}

#[get("/docker/delete-image-modal")]
pub(in crate::routers::ui::pages) async fn delete_image_modal(
    req: HttpRequest,
    query: web::Query<DeleteImageModalQuery>,
    config: web::Data<JwtConfig>,
) -> impl Responder {
    if !is_ui_authenticated(&req, &config) {
        return ui_login_redirect();
    }

    HttpResponse::Ok()
        .content_type(actix_web::http::header::ContentType::html())
        .body(render_delete_image_modal(&query))
}

#[get("/docker/delete-image-modal/empty")]
pub(in crate::routers::ui::pages) async fn empty_delete_image_modal(
    req: HttpRequest,
    config: web::Data<JwtConfig>,
) -> impl Responder {
    if !is_ui_authenticated(&req, &config) {
        return ui_login_redirect();
    }

    HttpResponse::Ok()
        .content_type(actix_web::http::header::ContentType::html())
        .body(empty_delete_image_modal_html())
}

#[post("/docker/delete-image")]
pub(in crate::routers::ui::pages) async fn delete_image(
    req: HttpRequest,
    form: web::Form<DeleteImageForm>,
    config: web::Data<JwtConfig>,
) -> impl Responder {
    if !is_ui_authenticated(&req, &config) {
        return ui_login_redirect();
    }

    if !validate_digest(&form.digest) {
        return HttpResponse::BadRequest().body("manifest deletion requires a digest reference");
    }

    let Some(repo_path) = repository_path(&form.repository) else {
        return HttpResponse::BadRequest().body("invalid repository name");
    };
    let Some(manifest_path) = manifest_path(&form.digest) else {
        return HttpResponse::BadRequest().body("invalid digest");
    };

    if !manifest_path.exists() {
        return HttpResponse::NotFound().body("manifest unknown");
    }

    if let Err(err) = tokio::fs::remove_file(&manifest_path).await {
        return HttpResponse::InternalServerError().body(err.to_string());
    }

    let tags_dir = repo_path.join("tags");
    if let Ok(mut entries) = tokio::fs::read_dir(&tags_dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await
            && let Ok(content) = tokio::fs::read_to_string(entry.path()).await
            && content.trim() == form.digest
        {
            let _ = tokio::fs::remove_file(entry.path()).await;
        }
    }

    HttpResponse::NoContent()
        .append_header((
            "HX-Redirect",
            with_base_path(&format!("/ui/docker/catalog?repo={}", form.repository)),
        ))
        .finish()
}

fn render_delete_image_modal(query: &DeleteImageModalQuery) -> String {
    let target = query
        .tag
        .as_deref()
        .filter(|tag| !tag.is_empty())
        .map(|tag| format!("{}:{tag}", query.repository))
        .unwrap_or_else(|| query.repository.clone());

    div()
        .attr("id", "confirm-delete-image-modal")
        .class("open")
        .child(
            button()
                .class("confirm-modal-backdrop")
                .attr("type", "button")
                .attr("hx-get", ui_path("/docker/delete-image-modal/empty"))
                .attr("hx-target", "#confirm-delete-image-modal")
                .attr("hx-swap", "outerHTML"),
        )
        .child(
            div()
                .class("confirm-modal-content")
                .child(
                    div()
                        .class("confirm-modal-header")
                        .child(div().class("confirm-modal-title").text("Confirm Delete"))
                        .child(
                            button()
                                .class("confirm-modal-close")
                                .attr("type", "button")
                                .attr("hx-get", ui_path("/docker/delete-image-modal/empty"))
                                .attr("hx-target", "#confirm-delete-image-modal")
                                .attr("hx-swap", "outerHTML")
                                .child(i().class("fa-solid fa-xmark")),
                        ),
                )
                .child(
                    div()
                        .class("confirm-modal-body")
                        .child(p().text("Are you sure you want to delete this image?"))
                        .child(div().class("confirm-delete-target").text(target))
                        .child(
                            form()
                                .class("confirm-actions")
                                .attr("hx-post", ui_path("/docker/delete-image"))
                                .attr("hx-target", "#confirm-delete-image-modal")
                                .attr("hx-swap", "outerHTML")
                                .child(
                                    input()
                                        .attr("type", "hidden")
                                        .attr("name", "repository")
                                        .attr("value", &query.repository),
                                )
                                .child(
                                    input()
                                        .attr("type", "hidden")
                                        .attr("name", "digest")
                                        .attr("value", &query.digest),
                                )
                                .child(
                                    button()
                                        .class("button cancel")
                                        .attr("type", "button")
                                        .attr("hx-get", ui_path("/docker/delete-image-modal/empty"))
                                        .attr("hx-target", "#confirm-delete-image-modal")
                                        .attr("hx-swap", "outerHTML")
                                        .text("Cancel"),
                                )
                                .child(
                                    button()
                                        .class("button delete")
                                        .attr("type", "submit")
                                        .text("Delete"),
                                ),
                        ),
                ),
        )
        .render()
}

fn empty_delete_image_modal_html() -> String {
    empty_delete_image_modal_element().render()
}

fn empty_delete_image_modal_element() -> Element {
    div().attr("id", "confirm-delete-image-modal")
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
        .cloned();

    let selected_meta = active_tag
        .as_ref()
        .and_then(|tag| tags_meta.iter().find(|m| &m.tag == tag));

    let left = div()
        .class("split-left panel")
        .child(
            div()
                .class("panel-title")
                .attr("data-i18n", "ui_repositories"),
        )
        .child(div().class("tree-scroll").child(render_repo_tree(
            &tree,
            repo.as_deref(),
            &tags_meta,
            active_tag.as_deref(),
        )));

    let right = div()
        .class("split-right panel")
        .child(render_metadata_panel(repo.as_deref(), selected_meta));

    render_page(
        HttpResponse::Ok(),
        content()
            .class("container-fluid py-4")
            .child(div().class("split-view").child(left).child(right))
            .child(empty_delete_image_modal_element()),
        UiPageKind::Docker,
    )
}

fn render_metadata_panel(repo: Option<&str>, selected_meta: Option<&TagMetadata>) -> Element {
    let title = match (repo, selected_meta) {
        (Some(_), Some(meta)) => div()
            .class("panel-title")
            .child(span().attr("data-i18n", "ui_metadata_for"))
            .child(span().text(format!(" {}", meta.tag))),
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
            ))
            .child(
                div().class("mt-4").child(
                    button()
                        .class("button-danger-sm")
                        .attr("type", "button")
                        .attr(
                            "hx-get",
                            format!(
                                "{}?repository={}&tag={}&digest={}",
                                ui_path("/docker/delete-image-modal"),
                                encode_query_component(repo.unwrap_or("")),
                                encode_query_component(&meta.tag),
                                encode_query_component(&meta.digest),
                            ),
                        )
                        .attr("hx-target", "#confirm-delete-image-modal")
                        .attr("hx-swap", "outerHTML")
                        .child(i().class("fas fa-trash mr-2"))
                        .child(
                            span()
                                .attr("data-i18n", "ui_delete_image")
                                .text("Delete Image"),
                        ),
                ),
            ),
        None => div()
            .class("empty")
            .attr("data-i18n", "ui_empty_select_tag"),
    };

    div()
        .class("h-100 d-flex flex-column")
        .child(title)
        .child(body)
}

fn meta_row(label_key: &str, value: &str) -> Element {
    div()
        .class("meta-row")
        .child(div().class("meta-label").attr("data-i18n", label_key))
        .child(div().class("meta-value mono").text(value))
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

fn render_repo_tree(
    root: &RepoTreeNode,
    selected_repo: Option<&str>,
    tags_meta: &[TagMetadata],
    active_tag: Option<&str>,
) -> Element {
    let mut tree = ul().class("repo-tree");
    for (name, child) in &root.children {
        tree = tree.child(render_repo_node(
            name,
            child,
            "",
            selected_repo,
            tags_meta,
            active_tag,
        ));
    }
    tree
}

fn render_repo_node(
    name: &str,
    node: &RepoTreeNode,
    parent_path: &str,
    selected_repo: Option<&str>,
    tags_meta: &[TagMetadata],
    active_tag: Option<&str>,
) -> Element {
    let item = li();
    let has_children = !node.children.is_empty();
    let is_repo = node.full_repo.is_some();
    let is_selected = node.full_repo.as_deref() == selected_repo;

    let full_path = if parent_path.is_empty() {
        name.to_string()
    } else {
        format!("{}/{}", parent_path, name)
    };

    // We only need details if there's something to expand (children or tags)
    let needs_details = has_children || is_selected;

    if !needs_details {
        if is_repo {
            let repo = node.full_repo.as_ref().unwrap();
            return item.child(
                div()
                    .class("tree-folder")
                    .child(i().class("fas fa-archive mr-2"))
                    .child(
                        a().attr(
                            "href",
                            format!("{}?repo={repo}", ui_path("/docker/catalog")),
                        )
                        .class("repo-link")
                        .text(name),
                    ),
            );
        } else {
            return item.child(span().class("tree-folder").text(name));
        }
    }

    let mut details = element("details").attr("data-path", &full_path);

    // Server-side hint for the currently active path
    if selected_repo.is_some_and(|selected| node_has_selected(node, selected)) || is_selected {
        details = details.attr("open", "open");
    }

    let mut summary = element("summary").class("tree-folder");
    if is_repo {
        let repo = node.full_repo.as_ref().unwrap();
        let class = if is_selected {
            "repo-link active"
        } else {
            "repo-link"
        };
        summary = summary.child(i().class("fas fa-archive mr-2")).child(
            a().attr(
                "href",
                format!("{}?repo={repo}", ui_path("/docker/catalog")),
            )
            .class(class)
            .text(name),
        );
    } else {
        summary = summary.text(name);
    }
    details = details.child(summary);

    if has_children {
        let mut children_list = ul().class("repo-tree");
        for (child_name, child_node) in &node.children {
            children_list = children_list.child(render_repo_node(
                child_name,
                child_node,
                &full_path,
                selected_repo,
                tags_meta,
                active_tag,
            ));
        }
        details = details.child(children_list);
    }

    if is_selected && !tags_meta.is_empty() {
        let mut tags_list = ul().class("tag-list");
        for meta in tags_meta {
            let repo = node.full_repo.as_ref().unwrap();
            let tag_class = if Some(meta.tag.as_str()) == active_tag {
                "tag-link active"
            } else {
                "tag-link"
            };
            tags_list = tags_list.child(
                li().child(
                    a().attr(
                        "href",
                        format!(
                            "{}?repo={repo}&tag={}",
                            ui_path("/docker/catalog"),
                            meta.tag
                        ),
                    )
                    .class(tag_class)
                    .child(i().class("fas fa-tag mr-2"))
                    .child(span().text(&meta.tag)),
                ),
            );
        }
        details = details.child(tags_list);
    }

    item.child(details)
}

fn node_has_selected(node: &RepoTreeNode, selected: &str) -> bool {
    if node.full_repo.as_deref() == Some(selected) {
        return true;
    }

    node.children
        .values()
        .any(|child| node_has_selected(child, selected))
}

fn encode_query_component(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}
