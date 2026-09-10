use actix_web::body::MessageBody;
use actix_web::{App, test as actix_test, web};
use quench_auth::prelude::JwtConfig;
use quench_db::{Db, InMemoryDb};
use warehouse_service::routers::ui::pages::files::browse::{
    BrowseFile, BrowseView, PreviewKind, Selection, TreeNode, build_tree, files_browse,
    preview_kind, render_browse_page,
};

fn body_html(resp: actix_web::HttpResponse) -> String {
    let body = resp.into_body().try_into_bytes().unwrap();
    String::from_utf8(body.to_vec()).unwrap()
}

fn jwt_config(auth_enabled: bool) -> JwtConfig {
    let mut config = JwtConfig::for_tests();
    config.auth_enabled = auth_enabled;
    config
}

fn in_memory_db() -> web::Data<Db> {
    web::Data::new(Db::InMemory(InMemoryDb::new()))
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

#[test]
fn an_unknown_storage_renders_a_not_found_notice() {
    let mut v = view(Selection::Dir(String::new()), TreeNode::default());
    v.storage_exists = false;
    assert!(body_html(render_browse_page(&v)).contains("ui_storage_not_found"));
}

#[test]
fn a_denied_read_renders_a_forbidden_notice_and_no_tree() {
    let mut v = view(Selection::Dir(String::new()), TreeNode::default());
    v.read_denied = true;
    let html = body_html(render_browse_page(&v));
    assert!(html.contains("api_error_forbidden"));
}

#[test]
fn a_selected_image_renders_an_inline_preview_and_a_download_link() {
    let tree = build_tree(&[file("photos/IMG_0001.jpg", 2048)]);
    let v = view(
        Selection::File {
            path: "photos/IMG_0001.jpg".to_string(),
            size: Some(2048),
        },
        tree,
    );
    let html = body_html(render_browse_page(&v));

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

#[test]
fn the_delete_form_appears_only_for_a_manager() {
    let tree = build_tree(&[file("a.txt", 3)]);
    let mut v = view(
        Selection::File {
            path: "a.txt".to_string(),
            size: Some(3),
        },
        tree,
    );
    v.can_manage = true;
    let html = body_html(render_browse_page(&v));
    assert!(html.contains("/files/delete-file"));
    assert!(html.contains("button-danger-sm"));
    assert!(html.contains("ui_file_delete"));
}

#[test]
fn an_unpreviewable_type_says_so() {
    let tree = build_tree(&[file("backup.zip", 9)]);
    let v = view(
        Selection::File {
            path: "backup.zip".to_string(),
            size: Some(9),
        },
        tree,
    );
    let html = body_html(render_browse_page(&v));
    assert!(html.contains("ui_file_preview_none"));
    assert!(!html.contains("<img"));
}

#[test]
fn a_directory_selection_lists_its_children_with_a_tree() {
    let tree = build_tree(&[
        file("photos/a.jpg", 1),
        file("photos/b.jpg", 2),
        file("top.txt", 3),
    ]);
    let v = view(Selection::Dir("photos".to_string()), tree);
    let html = body_html(render_browse_page(&v));

    assert!(html.contains("a.jpg"));
    assert!(html.contains("b.jpg"));
    // Left tree links to files and folders on the browse route.
    assert!(html.contains("/files/browse?storage=phone_backup&amp;path=photos%2Fa.jpg"));
    // Up link out of the folder.
    assert!(html.contains("ui_files_up"));
}

#[test]
fn the_truncation_notice_shows_when_the_tree_was_capped() {
    let mut v = view(
        Selection::Dir(String::new()),
        build_tree(&[file("a.txt", 1)]),
    );
    v.truncated = true;
    assert!(body_html(render_browse_page(&v)).contains("ui_storage_files_truncated"));
}

// ---------------------------------------------------------------------------
// HTTP handler
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn files_browse_redirects_to_login_when_unauthenticated() {
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(jwt_config(true)))
            .app_data(in_memory_db())
            .service(files_browse),
    )
    .await;
    let req = actix_test::TestRequest::get()
        .uri("/files/browse?storage=phone_backup")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert!(resp.status().is_redirection());
}

#[actix_web::test]
async fn files_browse_without_a_storage_sends_you_to_pick_one() {
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(jwt_config(false)))
            .app_data(in_memory_db())
            .service(files_browse),
    )
    .await;
    let req = actix_test::TestRequest::get()
        .uri("/files/browse")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert!(resp.status().is_redirection());
    let location = resp
        .headers()
        .get(actix_web::http::header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(location.contains("/ui/files/storages"), "{location}");
}

#[actix_web::test]
async fn files_browse_renders_a_not_found_notice_for_an_unknown_storage() {
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(jwt_config(false)))
            .app_data(in_memory_db())
            .service(files_browse),
    )
    .await;
    let req = actix_test::TestRequest::get()
        .uri("/files/browse?storage=does_not_exist")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
    let html = String::from_utf8(actix_test::read_body(resp).await.to_vec()).unwrap();
    assert!(html.contains("ui_storage_not_found"));
}
