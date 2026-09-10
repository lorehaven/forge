//! The file browser: a navigable tree of one storage's contents on the left,
//! an in-place preview of the selected file on the right.
//!
//! This is the page the storage-management view used to carry as a flat,
//! first-200 list. It is read-first - anyone with a realm session for this
//! service may browse and preview - and the one mutating control it offers
//! (delete a file) reuses [`super::storages::delete_file`], held to the same
//! `warehouse:write` bar every other management mutation is.
//!
//! The tree is built from a capped slice of the storage's files
//! ([`TREE_FILE_CAP`]); a storage larger than that shows the first slice and
//! says so, the same compromise the old list made. The preview pane points an
//! `<img>`/`<video>`/`<audio>`/`<iframe>` at the download endpoint with
//! `?disposition=inline`, which serves the bytes with a guessed `Content-Type`
//! for the browser to render rather than save.

use crate::domain::storage;
use crate::domain::storage_file;
use crate::routers::files::authz::can_on_storage_claims;
use crate::routers::files::ops::download::content_type_for;
use crate::routers::ui::authz::{can_manage, ui_claims};
use crate::routers::ui::common::{
    UiPageKind, is_ui_authenticated, render_page, ui_login_redirect, ui_path,
};
use actix_web::{HttpRequest, HttpResponse, Responder, get, web};
use quench_auth::prelude::JwtConfig;
use quench_db::prelude::Db;
use quench_starter::prelude::with_base_path;
use quench_web::prelude::*;
use quench_web_components::containers::empty_state;
use std::collections::BTreeMap;
use std::path::Path;

const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
const MIB: f64 = 1024.0 * 1024.0;

/// How many files the tree is built from before it stops and says there are
/// more. Browsing here is a convenience, not the backup client's paged
/// `GET /api/v1/files/{s}`.
const TREE_FILE_CAP: usize = 2000;

// ---------------------------------------------------------------------------
// Query
// ---------------------------------------------------------------------------

#[derive(serde::Deserialize)]
pub struct BrowseQuery {
    pub storage: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
}

// ---------------------------------------------------------------------------
// View model (kept free of `Db` so `render_browse_page` is a pure function)
// ---------------------------------------------------------------------------

/// One file, as fed to [`build_tree`].
pub struct BrowseFile {
    pub path: String,
    pub size: Option<i64>,
}

/// A node in the storage's directory tree. A leaf with `is_file` set is a
/// file; anything else is a directory (which may still be empty if the tree
/// was built from a static walk that hit its cap mid-directory).
#[derive(Default)]
pub struct TreeNode {
    pub children: BTreeMap<String, TreeNode>,
    pub is_file: bool,
    pub size: Option<i64>,
}

/// What `?path=` resolved to within the tree.
pub enum Selection {
    /// A directory - the storage root when the string is empty.
    Dir(String),
    /// A file, previewable or not.
    File { path: String, size: Option<i64> },
    /// `?path=` named nothing in the (possibly capped) tree.
    Missing(String),
}

pub struct BrowseView {
    pub storage: String,
    pub storage_exists: bool,
    pub read_denied: bool,
    pub can_manage: bool,
    pub tree: TreeNode,
    pub selection: Selection,
    pub truncated: bool,
}

/// Which element renders a file in place, chosen from the same guessed
/// `Content-Type` the inline download serves it with.
#[derive(Debug, PartialEq, Eq)]
pub enum PreviewKind {
    Image,
    Video,
    Audio,
    Pdf,
    Text,
    None,
}

pub fn preview_kind(path: &str) -> PreviewKind {
    let content_type = content_type_for(path);
    if content_type.starts_with("image/") {
        PreviewKind::Image
    } else if content_type.starts_with("video/") {
        PreviewKind::Video
    } else if content_type.starts_with("audio/") {
        PreviewKind::Audio
    } else if content_type == "application/pdf" {
        PreviewKind::Pdf
    } else if content_type.starts_with("text/") || content_type == "application/json" {
        PreviewKind::Text
    } else {
        PreviewKind::None
    }
}

// ---------------------------------------------------------------------------
// GET /ui/files/browse
// ---------------------------------------------------------------------------

#[get("/files/browse")]
pub async fn files_browse(
    req: HttpRequest,
    query: web::Query<BrowseQuery>,
    config: web::Data<JwtConfig>,
    db: web::Data<Db>,
) -> impl Responder {
    handle(&req, &query, &config, &db).await
}

#[get("/files/browse/")]
pub async fn files_browse_slash(
    req: HttpRequest,
    query: web::Query<BrowseQuery>,
    config: web::Data<JwtConfig>,
    db: web::Data<Db>,
) -> impl Responder {
    handle(&req, &query, &config, &db).await
}

async fn handle(
    req: &HttpRequest,
    query: &BrowseQuery,
    config: &JwtConfig,
    db: &Db,
) -> HttpResponse {
    if !is_ui_authenticated(req, config).await {
        return ui_login_redirect();
    }

    let claims = ui_claims(req, config).await;
    let manage = claims.as_ref().is_some_and(can_manage);

    let Some(name) = query.storage.as_deref().filter(|s| !s.is_empty()) else {
        // Nothing to browse without a storage - the storages page is where one
        // is picked.
        return HttpResponse::Found()
            .append_header(("Location", with_base_path("/ui/files/storages")))
            .finish();
    };
    let path = normalize_path(query.path.as_deref().unwrap_or(""));

    let dynamic = if crate::routers::files_enabled() {
        storage::list(db).await.unwrap_or_default()
    } else {
        Vec::new()
    };
    let dyn_storage = dynamic.iter().find(|s| s.name == name).cloned();
    let static_storage = crate::routers::files::storage(name);

    let mut view = BrowseView {
        storage: name.to_string(),
        storage_exists: dyn_storage.is_some() || static_storage.is_some(),
        read_denied: false,
        can_manage: manage,
        tree: TreeNode::default(),
        selection: Selection::Dir(path.clone()),
        truncated: false,
    };

    if let Some(dynamic_storage) = &dyn_storage {
        let read_ok = claims
            .as_ref()
            .is_some_and(|c| can_on_storage_claims(c, dynamic_storage, "read"));
        if !read_ok {
            view.read_denied = true;
        } else {
            let (files, truncated) = dynamic_files(db, name).await;
            view.tree = build_tree(&files);
            view.truncated = truncated;
            view.selection = resolve(&view.tree, &path);
        }
    } else if let Some(static_storage) = static_storage {
        // A static storage's read is open to any realm session, the same bar
        // this page as a whole is behind.
        let (files, truncated) = static_files(&static_storage.root).await;
        view.tree = build_tree(&files);
        view.truncated = truncated;
        view.selection = resolve(&view.tree, &path);
    }

    render_browse_page(&view)
}

/// Trim, drop `.`/`..`/empty segments, rejoin with `/`. The result only ever
/// indexes the in-memory tree and seeds an API URL that revalidates the path
/// itself, so this is a tidy-up, not the security boundary.
fn normalize_path(raw: &str) -> String {
    raw.split('/')
        .filter(|segment| !segment.is_empty() && *segment != "." && *segment != "..")
        .collect::<Vec<_>>()
        .join("/")
}

async fn dynamic_files(db: &Db, name: &str) -> (Vec<BrowseFile>, bool) {
    let rows = storage_file::list_files_page(db, name, "", None, TREE_FILE_CAP as i64 + 1, false)
        .await
        .unwrap_or_default();
    let truncated = rows.len() > TREE_FILE_CAP;
    let files = rows
        .into_iter()
        .take(TREE_FILE_CAP)
        .map(|file| BrowseFile {
            path: file.path,
            size: Some(file.size),
        })
        .collect();
    (files, truncated)
}

/// A bounded, depth-first walk of a static storage's root. `.part` staging
/// files are skipped, matching the JSON listing; empty directories are simply
/// absent from the tree, which is built from file paths.
async fn static_files(root: &Path) -> (Vec<BrowseFile>, bool) {
    let mut out: Vec<BrowseFile> = Vec::new();
    let mut truncated = false;
    let mut stack: Vec<(std::path::PathBuf, String)> = vec![(root.to_path_buf(), String::new())];

    while let Some((dir, rel)) = stack.pop() {
        let Ok(mut reader) = tokio::fs::read_dir(&dir).await else {
            continue;
        };
        while let Ok(Some(entry)) = reader.next_entry().await {
            if out.len() >= TREE_FILE_CAP {
                truncated = true;
                break;
            }
            let file_name = entry.file_name().to_string_lossy().to_string();
            if file_name.starts_with('.') && file_name.ends_with(".part") {
                continue;
            }
            let child_rel = if rel.is_empty() {
                file_name.clone()
            } else {
                format!("{rel}/{file_name}")
            };
            let Ok(metadata) = entry.metadata().await else {
                continue;
            };
            if metadata.is_dir() {
                stack.push((entry.path(), child_rel));
            } else if metadata.is_file() {
                out.push(BrowseFile {
                    path: child_rel,
                    size: Some(metadata.len() as i64),
                });
            }
        }
        if out.len() >= TREE_FILE_CAP {
            truncated = true;
            break;
        }
    }

    (out, truncated)
}

// ---------------------------------------------------------------------------
// Tree building / selection
// ---------------------------------------------------------------------------

pub fn build_tree(files: &[BrowseFile]) -> TreeNode {
    let mut root = TreeNode::default();
    for file in files {
        let segments: Vec<&str> = file.path.split('/').filter(|s| !s.is_empty()).collect();
        let mut node = &mut root;
        for (index, segment) in segments.iter().enumerate() {
            node = node.children.entry((*segment).to_string()).or_default();
            if index + 1 == segments.len() {
                node.is_file = true;
                node.size = file.size;
            }
        }
    }
    root
}

fn resolve(tree: &TreeNode, path: &str) -> Selection {
    if path.is_empty() {
        return Selection::Dir(String::new());
    }
    let mut node = tree;
    for segment in path.split('/').filter(|s| !s.is_empty()) {
        match node.children.get(segment) {
            Some(child) => node = child,
            None => return Selection::Missing(path.to_string()),
        }
    }
    if node.is_file && node.children.is_empty() {
        Selection::File {
            path: path.to_string(),
            size: node.size,
        }
    } else {
        Selection::Dir(path.to_string())
    }
}

fn node_at<'a>(tree: &'a TreeNode, path: &str) -> Option<&'a TreeNode> {
    let mut node = tree;
    for segment in path.split('/').filter(|s| !s.is_empty()) {
        node = node.children.get(segment)?;
    }
    Some(node)
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

pub fn render_browse_page(view: &BrowseView) -> HttpResponse {
    let left = div()
        .class("split-left panel")
        .child(
            div().class("panel-title").child(
                a().class("browse-back")
                    .attr(
                        "href",
                        format!(
                            "{}?storage={}",
                            ui_path("/files/storages"),
                            encode_query_component(&view.storage)
                        ),
                    )
                    .child(i().class("fas fa-arrow-left mr-2"))
                    .child(span().text(&view.storage)),
            ),
        )
        .child(div().class("tree-scroll").child(render_tree(view)));

    let right = div().class("split-right panel").child(render_detail(view));

    render_page(
        HttpResponse::Ok(),
        content()
            .class("container-fluid py-4")
            .child(div().class("split-view").child(left).child(right)),
        UiPageKind::Files,
    )
}

fn render_tree(view: &BrowseView) -> Element {
    if !view.storage_exists {
        return empty_state("ui_storage_not_found");
    }
    if view.read_denied {
        return empty_state("api_error_forbidden");
    }

    let selected = match &view.selection {
        Selection::Dir(path) => path.as_str(),
        Selection::File { path, .. } => path.as_str(),
        Selection::Missing(path) => path.as_str(),
    };

    let mut wrapper = div();

    // The storage root itself, as a selectable crumb above the tree.
    wrapper = wrapper.child(
        ul().class("repo-tree").child(
            li().child(
                a().attr(
                    "href",
                    format!(
                        "{}?storage={}",
                        ui_path("/files/browse"),
                        encode_query_component(&view.storage)
                    ),
                )
                .class(if selected.is_empty() {
                    "repo-link active"
                } else {
                    "repo-link"
                })
                .child(i().class("fas fa-hard-drive mr-2"))
                .child(
                    span()
                        .attr("data-i18n", "ui_storage_files_title")
                        .text("Files"),
                ),
            ),
        ),
    );

    if view.tree.children.is_empty() {
        wrapper = wrapper.child(empty_state("ui_files_empty_dir"));
    } else {
        let mut list = ul().class("repo-tree");
        for (name, node) in sorted_children(&view.tree) {
            list = list.child(render_tree_node(&view.storage, name, node, "", selected));
        }
        wrapper = wrapper.child(list);
    }

    if view.truncated {
        wrapper = wrapper.child(
            div()
                .class("file-truncated")
                .attr("data-i18n", "ui_storage_files_truncated")
                .text("Showing the first page of files only."),
        );
    }

    wrapper
}

/// Directories first, then files, each group by name.
fn sorted_children(node: &TreeNode) -> Vec<(&String, &TreeNode)> {
    let mut children: Vec<(&String, &TreeNode)> = node.children.iter().collect();
    children.sort_by(|(a_name, a_node), (b_name, b_node)| {
        a_node
            .is_file
            .cmp(&b_node.is_file)
            .then_with(|| a_name.cmp(b_name))
    });
    children
}

fn render_tree_node(
    storage: &str,
    name: &str,
    node: &TreeNode,
    parent_path: &str,
    selected: &str,
) -> Element {
    let full_path = if parent_path.is_empty() {
        name.to_string()
    } else {
        format!("{parent_path}/{name}")
    };
    let is_selected = full_path == selected;
    let href = format!(
        "{}?storage={}&path={}",
        ui_path("/files/browse"),
        encode_query_component(storage),
        encode_query_component(&full_path)
    );

    if node.is_file && node.children.is_empty() {
        return li().child(
            div()
                .class("tree-folder")
                .child(i().class(file_icon(name)))
                .child(
                    a().attr("href", href)
                        .class(if is_selected {
                            "repo-link active"
                        } else {
                            "repo-link"
                        })
                        .text(name),
                ),
        );
    }

    if node.children.is_empty() {
        return li().child(
            div()
                .class("tree-folder")
                .child(i().class("fas fa-folder mr-2"))
                .child(
                    a().attr("href", href)
                        .class(if is_selected {
                            "repo-link active"
                        } else {
                            "repo-link"
                        })
                        .text(name),
                ),
        );
    }

    let mut details = element("details").attr("data-path", &full_path);
    let on_path = selected == full_path || selected.starts_with(&format!("{full_path}/"));
    if on_path {
        details = details.attr("open", "open");
    }

    let summary = element("summary")
        .class("tree-folder")
        .child(i().class("fas fa-folder mr-2"))
        .child(
            a().attr("href", href)
                .class(if is_selected {
                    "repo-link active"
                } else {
                    "repo-link"
                })
                .text(name),
        );
    details = details.child(summary);

    let mut list = ul().class("repo-tree");
    for (child_name, child_node) in sorted_children(node) {
        list = list.child(render_tree_node(
            storage, child_name, child_node, &full_path, selected,
        ));
    }
    details = details.child(list);

    li().child(details)
}

fn render_detail(view: &BrowseView) -> Element {
    if !view.storage_exists {
        return detail_shell(
            div()
                .class("panel-title")
                .attr("data-i18n", "ui_storage_files_title"),
            empty_state("ui_storage_not_found"),
        );
    }
    if view.read_denied {
        return detail_shell(
            div()
                .class("panel-title")
                .attr("data-i18n", "ui_storage_files_title"),
            empty_state("api_error_forbidden"),
        );
    }

    match &view.selection {
        Selection::Missing(_) => detail_shell(
            div()
                .class("panel-title")
                .attr("data-i18n", "ui_storage_files_title"),
            empty_state("api_error_file_not_found"),
        ),
        Selection::Dir(path) => {
            let title = div()
                .class("panel-title")
                .attr("data-i18n", "ui_storage_files_title");
            let body = div()
                .class("d-flex flex-column")
                .attr("style", "gap:0.75rem")
                .child(render_breadcrumbs(&view.storage, path))
                .child(render_dir_listing(view, path));
            detail_shell(title, body)
        }
        Selection::File { path, size } => {
            let title = div()
                .class("panel-title")
                .child(span().attr("data-i18n", "ui_metadata_for"))
                .child(span().text(format!(" {}", last_segment(path))));
            detail_shell(title, render_file_detail(view, path, *size))
        }
    }
}

fn detail_shell(title: Element, body: Element) -> Element {
    div()
        .class("h-100 d-flex flex-column")
        .child(title)
        .child(div().class("manage-scroll").child(body))
}

fn render_breadcrumbs(storage: &str, path: &str) -> Element {
    let mut crumbs = div().class("browse-breadcrumbs").child(
        a().attr(
            "href",
            format!(
                "{}?storage={}",
                ui_path("/files/browse"),
                encode_query_component(storage)
            ),
        )
        .child(i().class("fas fa-hard-drive mr-2"))
        .child(span().text(storage)),
    );

    let mut accumulated = String::new();
    for segment in path.split('/').filter(|s| !s.is_empty()) {
        if accumulated.is_empty() {
            accumulated = segment.to_string();
        } else {
            accumulated = format!("{accumulated}/{segment}");
        }
        crumbs = crumbs
            .child(span().class("browse-crumb-sep").text(" / "))
            .child(
                a().attr(
                    "href",
                    format!(
                        "{}?storage={}&path={}",
                        ui_path("/files/browse"),
                        encode_query_component(storage),
                        encode_query_component(&accumulated)
                    ),
                )
                .text(segment),
            );
    }

    crumbs
}

fn render_dir_listing(view: &BrowseView, path: &str) -> Element {
    let Some(node) = node_at(&view.tree, path) else {
        return empty_state("api_error_file_not_found");
    };

    let mut section = div();

    if !path.is_empty() {
        let parent = match path.rsplit_once('/') {
            Some((parent, _)) => parent.to_string(),
            None => String::new(),
        };
        section = section.child(
            a().class("browse-up")
                .attr(
                    "href",
                    format!(
                        "{}?storage={}&path={}",
                        ui_path("/files/browse"),
                        encode_query_component(&view.storage),
                        encode_query_component(&parent)
                    ),
                )
                .child(i().class("fas fa-arrow-turn-up mr-2"))
                .child(span().attr("data-i18n", "ui_files_up").text("Up")),
        );
    }

    if node.children.is_empty() {
        return section.child(empty_state("ui_files_empty_dir"));
    }

    let mut list = ul().class("file-list");
    for (name, child) in sorted_children(node) {
        let child_path = if path.is_empty() {
            name.to_string()
        } else {
            format!("{path}/{name}")
        };
        let href = format!(
            "{}?storage={}&path={}",
            ui_path("/files/browse"),
            encode_query_component(&view.storage),
            encode_query_component(&child_path)
        );
        let icon = if child.is_file {
            file_icon(name)
        } else {
            "fas fa-folder mr-2"
        };
        let size = match (child.is_file, child.size) {
            (true, Some(bytes)) => format_bytes(bytes),
            _ => String::new(),
        };
        list = list.child(
            li().class("file-row")
                .child(i().class(icon))
                .child(a().class("file-name").attr("href", href).text(name))
                .child(span().class("file-size mono").text(size)),
        );
    }

    section.child(list)
}

fn render_file_detail(view: &BrowseView, path: &str, size: Option<i64>) -> Element {
    let content_type = content_type_for(path);
    let size_text = size.map(format_bytes).unwrap_or_else(|| "—".to_string());

    let meta = div()
        .class("meta-list")
        .child(meta_row("ui_files_col_name", last_segment(path)))
        .child(meta_row("ui_storage_root", path))
        .child(meta_row("ui_files_col_type", content_type))
        .child(meta_row("ui_files_col_size", &size_text));

    let download_href = with_base_path(&format!(
        "/api/v1/files/{}/file?path={}",
        view.storage,
        encode_query_component(path)
    ));

    let mut actions = div().class("file-actions-row").child(
        a().class("button-neutral-sm")
            .attr("href", download_href)
            .attr("download", last_segment(path))
            .child(i().class("fas fa-download mr-2"))
            .child(
                span()
                    .attr("data-i18n", "ui_file_download")
                    .text("Download"),
            ),
    );

    if view.can_manage {
        actions = actions.child(
            form()
                .class("inline-action-form")
                .attr("hx-post", ui_path("/files/delete-file"))
                .attr("hx-swap", "none")
                .child(
                    input()
                        .attr("type", "hidden")
                        .attr("name", "storage")
                        .attr("value", &view.storage),
                )
                .child(
                    input()
                        .attr("type", "hidden")
                        .attr("name", "path")
                        .attr("value", path),
                )
                .child(
                    button()
                        .class("button-danger-sm")
                        .attr("type", "submit")
                        .attr("data-i18n", "ui_file_delete")
                        .text("Delete"),
                ),
        );
    }

    div()
        .child(meta)
        .child(render_preview(&view.storage, path))
        .child(actions)
}

fn render_preview(storage: &str, path: &str) -> Element {
    let inline_url = with_base_path(&format!(
        "/api/v1/files/{}/file?path={}&disposition=inline",
        storage,
        encode_query_component(path)
    ));
    let name = last_segment(path);

    let media = match preview_kind(path) {
        PreviewKind::Image => element("img")
            .class("file-preview-media")
            .attr("src", &inline_url)
            .attr("alt", name)
            .attr("loading", "lazy"),
        PreviewKind::Video => element("video")
            .class("file-preview-media")
            .attr("src", &inline_url)
            .attr("controls", "controls")
            .attr("preload", "metadata"),
        PreviewKind::Audio => element("audio")
            .class("file-preview-audio")
            .attr("src", &inline_url)
            .attr("controls", "controls")
            .attr("preload", "metadata"),
        PreviewKind::Pdf | PreviewKind::Text => element("iframe")
            .class("file-preview-frame")
            .attr("src", &inline_url)
            .attr("title", name),
        PreviewKind::None => {
            return div()
                .class("file-preview")
                .child(empty_state("ui_file_preview_none"));
        }
    };

    div().class("file-preview").child(media)
}

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

fn last_segment(path: &str) -> &str {
    path.rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(path)
}

fn file_icon(name: &str) -> &'static str {
    match preview_kind(name) {
        PreviewKind::Image => "fas fa-file-image mr-2",
        PreviewKind::Video => "fas fa-file-video mr-2",
        PreviewKind::Audio => "fas fa-file-audio mr-2",
        PreviewKind::Pdf => "fas fa-file-pdf mr-2",
        PreviewKind::Text => "fas fa-file-lines mr-2",
        PreviewKind::None => "fas fa-file mr-2",
    }
}

fn format_bytes(bytes: i64) -> String {
    let b = bytes as f64;
    if b >= GIB {
        format!("{:.2} GiB", b / GIB)
    } else if b >= MIB {
        format!("{:.2} MiB", b / MIB)
    } else if b >= 1024.0 {
        format!("{:.2} KiB", b / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

fn meta_row(label_key: &str, value: &str) -> Element {
    div()
        .class("meta-row")
        .child(div().class("meta-label").attr("data-i18n", label_key))
        .child(div().class("meta-value mono").child(span().text(value)))
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
