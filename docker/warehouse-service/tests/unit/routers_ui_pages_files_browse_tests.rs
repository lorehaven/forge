use crate::support;

use http::{Method, StatusCode};
use http_body_util::BodyExt;
use quench_auth::domain::jwt::JwtConfig;
use quench_db::InMemoryDb;
use quench_db::prelude::Db;
use quench_http::endpoint::Endpoint;
use warehouse_service::routers::ui::pages::files::browse::{
    BrowseFile, BrowseView, PreviewKind, Selection, TreeNode, build_tree, preview_kind,
    render_browse_page,
};

async fn body_html(resp: quench_http::response::Response) -> String {
    let collected = resp.into_hyper().into_body().collect().await.unwrap();
    String::from_utf8(collected.to_bytes().to_vec()).unwrap()
}

fn jwt_config(auth_enabled: bool) -> JwtConfig {
    let mut config = JwtConfig::for_tests();
    config.auth_enabled = auth_enabled;
    config
}

fn in_memory_db() -> Db {
    Db::InMemory(InMemoryDb::new())
}

fn file(path: &str, size: i64) -> BrowseFile {
    BrowseFile {
        path: path.to_string(),
        size: Some(size),
    }
}

fn view(selection: Selection, tree: TreeNode) -> BrowseView {
    BrowseView {
        storage: "phone_backup".to_string(),
        storage_exists: true,
        read_denied: false,
        can_manage: false,
        tree,
        selection,
        truncated: false,
    }
}

// ---------------------------------------------------------------------------
// build_tree / preview_kind
// ---------------------------------------------------------------------------

#[test]
fn build_tree_nests_paths_and_marks_leaves_as_files() {
    let tree = build_tree(&[
        file("photos/2024/a.jpg", 10),
        file("photos/2024/b.jpg", 20),
        file("notes.txt", 5),
    ]);

    let photos = tree.children.get("photos").expect("photos dir");
    assert!(!photos.is_file);
    let year = photos.children.get("2024").expect("2024 dir");
    assert_eq!(year.children.len(), 2);
    assert!(year.children.get("a.jpg").unwrap().is_file);
    assert_eq!(year.children.get("a.jpg").unwrap().size, Some(10));
    assert!(tree.children.get("notes.txt").unwrap().is_file);
}

#[test]
fn preview_kind_classifies_by_extension() {
    assert_eq!(preview_kind("a/b/IMG.JPG"), PreviewKind::Image);
    assert_eq!(preview_kind("clip.mp4"), PreviewKind::Video);
    assert_eq!(preview_kind("song.flac"), PreviewKind::Audio);
    assert_eq!(preview_kind("manual.pdf"), PreviewKind::Pdf);
    assert_eq!(preview_kind("readme.md"), PreviewKind::Text);
    assert_eq!(preview_kind("data.json"), PreviewKind::Text);
    assert_eq!(preview_kind("archive.zip"), PreviewKind::None);
    assert_eq!(preview_kind("no_extension"), PreviewKind::None);
}

// ---------------------------------------------------------------------------
// render_browse_page
// ---------------------------------------------------------------------------

#[tokio::test]
async fn an_unknown_storage_renders_a_not_found_notice() {
    let mut v = view(Selection::Dir(String::new()), TreeNode::default());
    v.storage_exists = false;
    assert!(
        body_html(render_browse_page(&v))
            .await
            .contains("ui_storage_not_found")
    );
}

#[tokio::test]
async fn a_denied_read_renders_a_forbidden_notice_and_no_tree() {
    let mut v = view(Selection::Dir(String::new()), TreeNode::default());
    v.read_denied = true;
    let html = body_html(render_browse_page(&v)).await;
    assert!(html.contains("api_error_forbidden"));
}

#[tokio::test]
async fn a_selected_image_renders_an_inline_preview_and_a_download_link() {
    let tree = build_tree(&[file("photos/IMG_0001.jpg", 2048)]);
    let v = view(
        Selection::File {
            path: "photos/IMG_0001.jpg".to_string(),
            size: Some(2048),
        },
        tree,
    );
    let html = body_html(render_browse_page(&v)).await;

    // Preview points at the inline download.
    assert!(html.contains("<img"));
    assert!(html.contains(
        "/api/v1/files/phone_backup/file?path=photos%2FIMG_0001.jpg&amp;disposition=inline"
    ));
    // Download reuses the danger-button shape as a neutral sibling.
    assert!(html.contains("button-neutral-sm"));
    assert!(html.contains("download=\"IMG_0001.jpg\""));
    assert!(html.contains("ui_file_download"));
    // Read-only: no delete control.
    assert!(!html.contains("/files/delete-file"));
}

#[tokio::test]
async fn the_delete_form_appears_only_for_a_manager() {
    let tree = build_tree(&[file("a.txt", 3)]);
    let mut v = view(
        Selection::File {
            path: "a.txt".to_string(),
            size: Some(3),
        },
        tree,
    );
    v.can_manage = true;
    let html = body_html(render_browse_page(&v)).await;
    assert!(html.contains("/files/delete-file"));
    assert!(html.contains("button-danger-sm"));
    assert!(html.contains("ui_file_delete"));
}

#[tokio::test]
async fn an_unpreviewable_type_says_so() {
    let tree = build_tree(&[file("backup.zip", 9)]);
    let v = view(
        Selection::File {
            path: "backup.zip".to_string(),
            size: Some(9),
        },
        tree,
    );
    let html = body_html(render_browse_page(&v)).await;
    assert!(html.contains("ui_file_preview_none"));
    assert!(!html.contains("<img"));
}

#[tokio::test]
async fn a_directory_selection_lists_its_children_with_a_tree() {
    let tree = build_tree(&[
        file("photos/a.jpg", 1),
        file("photos/b.jpg", 2),
        file("top.txt", 3),
    ]);
    let v = view(Selection::Dir("photos".to_string()), tree);
    let html = body_html(render_browse_page(&v)).await;

    assert!(html.contains("a.jpg"));
    assert!(html.contains("b.jpg"));
    // Left tree links to files and folders on the browse route.
    assert!(html.contains("/files/browse?storage=phone_backup&amp;path=photos%2Fa.jpg"));
    // Up link out of the folder.
    assert!(html.contains("ui_files_up"));
}

#[tokio::test]
async fn the_truncation_notice_shows_when_the_tree_was_capped() {
    let mut v = view(
        Selection::Dir(String::new()),
        build_tree(&[file("a.txt", 1)]),
    );
    v.truncated = true;
    assert!(
        body_html(render_browse_page(&v))
            .await
            .contains("ui_storage_files_truncated")
    );
}

// ---------------------------------------------------------------------------
// HTTP handler
// ---------------------------------------------------------------------------

async fn app(
    jwt_config: JwtConfig,
    db: Db,
) -> (
    std::sync::Arc<dyn Endpoint>,
    std::sync::Arc<quench_http::di::Container>,
) {
    warehouse_service::routers::ui::pages::files::browse::register_routes();
    let container = support::container_builder()
        .provide(jwt_config)
        .provide(db)
        .build()
        .await
        .unwrap();
    support::app(container).await
}

#[tokio::test]
async fn files_browse_redirects_to_login_when_unauthenticated() {
    let (app, container) = app(jwt_config(true), in_memory_db()).await;
    let resp = app
        .call(support::req(
            Method::GET,
            "/ui/files/browse?storage=phone_backup",
            &container,
        ))
        .await;
    assert!(resp.status().is_redirection());
}

#[tokio::test]
async fn files_browse_without_a_storage_sends_you_to_pick_one() {
    let (app, container) = app(jwt_config(false), in_memory_db()).await;
    let resp = app
        .call(support::req(Method::GET, "/ui/files/browse", &container))
        .await;
    assert!(resp.status().is_redirection());
    let (headers, _) = support::parts(resp).await;
    let location = support::location(&headers);
    assert!(location.contains("/ui/files/storages"), "{location}");
}

#[tokio::test]
async fn files_browse_renders_a_not_found_notice_for_an_unknown_storage() {
    let (app, container) = app(jwt_config(false), in_memory_db()).await;
    let resp = app
        .call(support::req(
            Method::GET,
            "/ui/files/browse?storage=does_not_exist",
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let html = body_html(resp).await;
    assert!(html.contains("ui_storage_not_found"));
}
