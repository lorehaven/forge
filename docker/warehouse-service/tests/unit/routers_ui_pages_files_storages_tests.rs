use crate::support;

use chrono::Utc;
use http::{Method, StatusCode};
use http_body_util::BodyExt;
use quench_auth::domain::jwt::JwtConfig;
use quench_db::InMemoryDb;
use quench_db::prelude::Db;
use quench_http::endpoint::Endpoint;
use warehouse_service::domain::storage::DynamicStorage;
use warehouse_service::routers::ui::pages::files::storages::{
    SelectedView, StoragesView, render_storages_page,
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

fn dynamic(name: &str, owner: &str) -> DynamicStorage {
    DynamicStorage {
        name: name.to_string(),
        owner: owner.to_string(),
        max_file_bytes: None,
        quota_bytes: 500 * 1024 * 1024 * 1024,
        used_bytes: 50 * 1024 * 1024 * 1024,
        sync_enabled: true,
        created_at: Utc::now(),
    }
}

// -----------------------------------------------------------------
// render_storages_page
// -----------------------------------------------------------------

#[tokio::test]
async fn empty_view_shows_the_empty_state() {
    let resp = render_storages_page(&StoragesView::default(), false);
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(body_html(resp).await.contains("ui_storages_empty"));
}

#[tokio::test]
async fn the_list_shows_static_and_dynamic_storages() {
    let view = StoragesView {
        static_names: vec!["artifacts".to_string()],
        dynamic: vec![dynamic("phone_backup", "losseheil")],
        selected: None,
    };
    let html = body_html(render_storages_page(&view, false)).await;
    assert!(html.contains("artifacts"));
    assert!(html.contains("ui_storage_static_badge"));
    assert!(html.contains("phone_backup"));
    assert!(html.contains("losseheil"));
}

#[tokio::test]
async fn a_selected_dynamic_storage_shows_owner_and_a_quota_bar() {
    let view = StoragesView {
        static_names: vec![],
        dynamic: vec![dynamic("phone_backup", "losseheil")],
        selected: Some(SelectedView {
            name: "phone_backup".to_string(),
            dynamic: Some(dynamic("phone_backup", "losseheil")),
            static_root: None,
            notice: None,
        }),
    };
    let html = body_html(render_storages_page(&view, false)).await;
    assert!(html.contains("ui_storage_owner"));
    assert!(html.contains("quota-bar"));
    assert!(html.contains("GiB"));
}

#[tokio::test]
async fn management_controls_appear_only_with_permission() {
    let selected = || SelectedView {
        name: "phone_backup".to_string(),
        dynamic: Some(dynamic("phone_backup", "losseheil")),
        static_root: None,
        notice: None,
    };
    let view = |sel| StoragesView {
        static_names: vec![],
        dynamic: vec![dynamic("phone_backup", "losseheil")],
        selected: Some(sel),
    };

    let with_manage = body_html(render_storages_page(&view(selected()), true)).await;
    assert!(with_manage.contains("ui_storage_edit_title"));
    assert!(with_manage.contains("ui_storage_new_title"));
    assert!(with_manage.contains("ui_storage_delete"));
    assert!(with_manage.contains("/files/storages/phone_backup/edit"));

    let without = body_html(render_storages_page(&view(selected()), false)).await;
    assert!(!without.contains("ui_storage_edit_title"));
    assert!(!without.contains("ui_storage_new_title"));
    assert!(!without.contains("/edit"));
}

fn selected_dynamic() -> StoragesView {
    StoragesView {
        static_names: vec![],
        dynamic: vec![dynamic("phone_backup", "losseheil")],
        selected: Some(SelectedView {
            name: "phone_backup".to_string(),
            dynamic: Some(dynamic("phone_backup", "losseheil")),
            static_root: None,
            notice: None,
        }),
    }
}

#[tokio::test]
async fn a_selected_storage_links_to_the_file_browser_instead_of_listing_files() {
    let html = body_html(render_storages_page(&selected_dynamic(), true)).await;
    assert!(html.contains("ui_browse_open"));
    assert!(html.contains("/files/browse?storage=phone_backup"));
    // The old in-panel list and its (broken) download link are gone.
    assert!(!html.contains("/download?path="));
    assert!(!html.contains("ui_storage_files_truncated"));

    // The link shows for a read-only viewer too - browsing is not a mutation.
    let html_ro = body_html(render_storages_page(&selected_dynamic(), false)).await;
    assert!(html_ro.contains("/files/browse?storage=phone_backup"));
}

#[tokio::test]
async fn a_notice_replaces_the_detail_panel_for_an_unknown_storage() {
    let view = StoragesView {
        static_names: vec!["artifacts".to_string()],
        dynamic: vec![],
        selected: Some(SelectedView {
            name: "missing".to_string(),
            dynamic: None,
            static_root: None,
            notice: Some("ui_storage_not_found"),
        }),
    };
    assert!(
        body_html(render_storages_page(&view, true))
            .await
            .contains("ui_storage_not_found")
    );
}

// -----------------------------------------------------------------
// HTTP handlers - file storage is off in the test binary, so the
// reachable branches are login-redirect and feature-disabled.
// -----------------------------------------------------------------

async fn app(
    jwt_config: JwtConfig,
    db: Db,
) -> (
    std::sync::Arc<dyn Endpoint>,
    std::sync::Arc<quench_http::di::Container>,
) {
    warehouse_service::routers::ui::pages::files::storages::register_routes();
    let container = support::container_builder()
        .provide(jwt_config)
        .provide(db)
        .build()
        .await
        .unwrap();
    support::app(container).await
}

#[tokio::test]
async fn files_storages_redirects_to_login_when_unauthenticated() {
    let (app, container) = app(jwt_config(true), in_memory_db()).await;
    let resp = app
        .call(support::req(Method::GET, "/ui/files/storages", &container))
        .await;
    assert!(resp.status().is_redirection());
}

#[tokio::test]
async fn files_storages_renders_when_authenticated() {
    let (app, container) = app(jwt_config(false), in_memory_db()).await;
    let resp = app
        .call(support::req(Method::GET, "/ui/files/storages", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn create_storage_redirects_to_login_without_a_session() {
    let (app, container) = app(jwt_config(true), in_memory_db()).await;
    let resp = app
        .call(support::form_req(
            Method::POST,
            "/ui/files/storages",
            &[
                ("name", "backups"),
                ("owner", "losseheil"),
                ("quota_gib", "10"),
            ],
            &container,
        ))
        .await;
    assert!(resp.status().is_redirection());
}

#[tokio::test]
async fn mutations_are_not_found_when_file_storage_is_disabled() {
    let (app, container) = app(jwt_config(false), in_memory_db()).await;

    for req in [
        support::form_req(
            Method::POST,
            "/ui/files/storages",
            &[
                ("name", "backups"),
                ("owner", "losseheil"),
                ("quota_gib", "10"),
            ],
            &container,
        ),
        support::form_req(
            Method::POST,
            "/ui/files/storages/backups/edit",
            &[("quota_gib", "20")],
            &container,
        ),
        support::form_req(
            Method::POST,
            "/ui/files/delete-storage",
            &[("name", "backups")],
            &container,
        ),
        support::form_req(
            Method::POST,
            "/ui/files/delete-file",
            &[("storage", "backups"), ("path", "a.txt")],
            &container,
        ),
    ] {
        let resp = app.call(req).await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }
}

#[tokio::test]
async fn the_delete_storage_modal_names_its_target() {
    let (app, container) = app(jwt_config(false), in_memory_db()).await;
    let resp = app
        .call(support::req(
            Method::GET,
            "/ui/files/delete-storage-modal?storage=phone_backup",
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let html = body_html(resp).await;
    assert!(html.contains("confirm-delete-storage-modal"));
    assert!(html.contains("phone_backup"));
    assert!(html.contains("/files/delete-storage"));
}
