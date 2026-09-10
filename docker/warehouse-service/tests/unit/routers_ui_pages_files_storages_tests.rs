use actix_web::body::MessageBody;
use actix_web::{App, test as actix_test, web};
use chrono::Utc;
use quench_auth::prelude::JwtConfig;
use quench_db::{Db, InMemoryDb};
use warehouse_service::domain::storage::DynamicStorage;
use warehouse_service::routers::ui::pages::files::storages::{
    SelectedView, StoragesView, create_storage, delete_file, delete_storage, delete_storage_modal,
    edit_storage, files_storages, render_storages_page,
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

#[test]
fn empty_view_shows_the_empty_state() {
    let resp = render_storages_page(&StoragesView::default(), false);
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
    assert!(body_html(resp).contains("ui_storages_empty"));
}

#[test]
fn the_list_shows_static_and_dynamic_storages() {
    let view = StoragesView {
        static_names: vec!["artifacts".to_string()],
        dynamic: vec![dynamic("phone_backup", "losseheil")],
        selected: None,
    };
    let html = body_html(render_storages_page(&view, false));
    assert!(html.contains("artifacts"));
    assert!(html.contains("ui_storage_static_badge"));
    assert!(html.contains("phone_backup"));
    assert!(html.contains("losseheil"));
}

#[test]
fn a_selected_dynamic_storage_shows_owner_and_a_quota_bar() {
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
    let html = body_html(render_storages_page(&view, false));
    assert!(html.contains("ui_storage_owner"));
    assert!(html.contains("quota-bar"));
    assert!(html.contains("GiB"));
}

#[test]
fn management_controls_appear_only_with_permission() {
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

    let with_manage = body_html(render_storages_page(&view(selected()), true));
    assert!(with_manage.contains("ui_storage_edit_title"));
    assert!(with_manage.contains("ui_storage_new_title"));
    assert!(with_manage.contains("ui_storage_delete"));
    assert!(with_manage.contains("/files/storages/phone_backup/edit"));

    let without = body_html(render_storages_page(&view(selected()), false));
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

#[test]
fn a_selected_storage_links_to_the_file_browser_instead_of_listing_files() {
    let html = body_html(render_storages_page(&selected_dynamic(), true));
    assert!(html.contains("ui_browse_open"));
    assert!(html.contains("/files/browse?storage=phone_backup"));
    // The old in-panel list and its (broken) download link are gone.
    assert!(!html.contains("/download?path="));
    assert!(!html.contains("ui_storage_files_truncated"));

    // The link shows for a read-only viewer too - browsing is not a mutation.
    let html_ro = body_html(render_storages_page(&selected_dynamic(), false));
    assert!(html_ro.contains("/files/browse?storage=phone_backup"));
}

#[test]
fn a_notice_replaces_the_detail_panel_for_an_unknown_storage() {
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
    assert!(body_html(render_storages_page(&view, true)).contains("ui_storage_not_found"));
}

// -----------------------------------------------------------------
// HTTP handlers - file storage is off in the test binary, so the
// reachable branches are login-redirect and feature-disabled.
// -----------------------------------------------------------------

#[actix_web::test]
async fn files_storages_redirects_to_login_when_unauthenticated() {
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(jwt_config(true)))
            .app_data(in_memory_db())
            .service(files_storages),
    )
    .await;
    let req = actix_test::TestRequest::get()
        .uri("/files/storages")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert!(resp.status().is_redirection());
}

#[actix_web::test]
async fn files_storages_renders_when_authenticated() {
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(jwt_config(false)))
            .app_data(in_memory_db())
            .service(files_storages),
    )
    .await;
    let req = actix_test::TestRequest::get()
        .uri("/files/storages")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
}

#[actix_web::test]
async fn create_storage_redirects_to_login_without_a_session() {
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(jwt_config(true)))
            .app_data(in_memory_db())
            .service(create_storage),
    )
    .await;
    let req = actix_test::TestRequest::post()
        .uri("/files/storages")
        .set_form([
            ("name", "backups"),
            ("owner", "losseheil"),
            ("quota_gib", "10"),
        ])
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert!(resp.status().is_redirection());
}

#[actix_web::test]
async fn mutations_are_not_found_when_file_storage_is_disabled() {
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(jwt_config(false)))
            .app_data(in_memory_db())
            .service(create_storage)
            .service(edit_storage)
            .service(delete_storage)
            .service(delete_file),
    )
    .await;

    for req in [
        actix_test::TestRequest::post()
            .uri("/files/storages")
            .set_form([
                ("name", "backups"),
                ("owner", "losseheil"),
                ("quota_gib", "10"),
            ])
            .to_request(),
        actix_test::TestRequest::post()
            .uri("/files/storages/backups/edit")
            .set_form([("quota_gib", "20")])
            .to_request(),
        actix_test::TestRequest::post()
            .uri("/files/delete-storage")
            .set_form([("name", "backups")])
            .to_request(),
        actix_test::TestRequest::post()
            .uri("/files/delete-file")
            .set_form([("storage", "backups"), ("path", "a.txt")])
            .to_request(),
    ] {
        let resp = actix_test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::NOT_FOUND);
    }
}

#[actix_web::test]
async fn the_delete_storage_modal_names_its_target() {
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(jwt_config(false)))
            .service(delete_storage_modal),
    )
    .await;
    let req = actix_test::TestRequest::get()
        .uri("/files/delete-storage-modal?storage=phone_backup")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
    let body = actix_test::read_body(resp).await;
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("confirm-delete-storage-modal"));
    assert!(html.contains("phone_backup"));
    assert!(html.contains("/files/delete-storage"));
}
