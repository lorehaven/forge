use crate::support::{self, WithRivets};
use chrono::Utc;
use http::{Method, StatusCode};
use http_body_util::BodyExt;
use quench_auth::domain::jwt::JwtConfig;
use quench_db::InMemoryDb;
use quench_db::prelude::{Crud, Db};
use quench_http::endpoint::Endpoint;
use sqlx::types::Json;
use std::sync::Arc;
use warehouse_service::domain::rivet::RivetPackage;
use warehouse_service::routers::ui::pages::home::render_home_page;
use warehouse_service::routers::ui::pages::rivets::catalog::render_rivets_page;

fn package(name: &str, version: &str, yanked: bool) -> RivetPackage {
    RivetPackage {
        id: RivetPackage::id_for(name, version),
        name: name.to_string(),
        version: version.to_string(),
        description: Some("a test overlay".to_string()),
        namespace: Some("demo-ns".to_string()),
        filename: format!("{name}-{version}.rivet"),
        size_bytes: 2048,
        sha256: "deadbeef".to_string(),
        manifest: Json(serde_json::json!({
            "package": {"name": name, "version": version},
            "requires": {"riveter": ">=0.3", "packages": ["postgres >=1", "redis"]},
            "meta": {"gpu": true, "owner": "ml-team"},
        })),
        uploaded_by: "ci".to_string(),
        yanked,
        created_at: Utc::now(),
    }
}

async fn body_html(resp: quench_http::response::Response) -> String {
    let collected = resp.into_hyper().into_body().collect().await.unwrap();
    String::from_utf8(collected.to_bytes().to_vec()).unwrap()
}

fn jwt_config(auth_enabled: bool) -> JwtConfig {
    let mut config = JwtConfig::for_tests();
    config.auth_enabled = auth_enabled;
    config
}

// -----------------------------------------------------------------
// render_rivets_page
// -----------------------------------------------------------------

#[tokio::test]
async fn render_with_no_packages_shows_the_empty_state() {
    let resp = render_rivets_page(&[], None, None, false);
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(body_html(resp).await.contains("ui_rivet_empty"));
}

#[tokio::test]
async fn render_lists_packages_in_the_tree_and_asks_for_a_selection() {
    let packages = vec![
        package("forge", "1.0.0", false),
        package("vllm", "2.0.0", false),
    ];
    let html = body_html(render_rivets_page(&packages, None, None, false)).await;
    assert!(html.contains("forge"));
    assert!(html.contains("vllm"));
    assert!(html.contains("ui_rivet_empty_select_version"));
}

#[tokio::test]
async fn render_opens_on_the_newest_version_by_semver_not_text_order() {
    let packages = vec![
        package("forge", "0.9.0", false),
        package("forge", "0.10.0", false),
        package("forge", "0.2.0", false),
    ];
    let html = body_html(render_rivets_page(&packages, Some("forge"), None, false)).await;
    assert!(html.contains("riveter install forge@0.10.0"), "{html}");

    // And the version list is ordered the same way.
    let tree_ten = html.find("tag=0.10.0").unwrap();
    let tree_nine = html.find("tag=0.9.0").unwrap();
    let tree_two = html.find("tag=0.2.0").unwrap();
    assert!(tree_ten < tree_nine && tree_nine < tree_two, "{html}");
}

#[tokio::test]
async fn render_prefers_an_offerable_version_over_a_yanked_newest() {
    let packages = vec![
        package("forge", "1.0.0", false),
        package("forge", "1.1.0", true),
    ];
    let html = body_html(render_rivets_page(&packages, Some("forge"), None, false)).await;
    assert!(html.contains("riveter install forge@1.0.0"), "{html}");

    // A yanked version is still selectable by name, and shown as such.
    let html = body_html(render_rivets_page(
        &packages,
        Some("forge"),
        Some("1.1.0"),
        false,
    ))
    .await;
    assert!(html.contains("riveter install forge@1.1.0"));
    assert!(html.contains("ui_status_yanked"));
}

#[tokio::test]
async fn render_falls_back_when_all_versions_are_yanked_or_the_name_is_unknown() {
    let packages = vec![package("forge", "1.0.0", true)];
    let html = body_html(render_rivets_page(&packages, Some("forge"), None, false)).await;
    assert!(html.contains("riveter install forge@1.0.0"));

    let html = body_html(render_rivets_page(&packages, Some("ghost"), None, false)).await;
    assert!(html.contains("ui_rivet_empty_select_version"));
}

#[tokio::test]
async fn render_shows_what_the_manifest_declares() {
    let packages = vec![package("forge", "1.0.0", false)];
    let html = body_html(render_rivets_page(
        &packages,
        Some("forge"),
        Some("1.0.0"),
        false,
    ))
    .await;

    assert!(html.contains("demo-ns"));
    assert!(html.contains("a test overlay"));
    assert!(html.contains("&gt;=0.3") || html.contains(">=0.3"));
    assert!(html.contains("postgres &gt;=1, redis") || html.contains("postgres >=1, redis"));
    // The package's own [meta] keys are shown under their own names, untranslated.
    assert!(html.contains("gpu"));
    assert!(html.contains("ml-team"));
    assert!(html.contains("deadbeef"));
    assert!(html.contains("2048 bytes"));
    assert!(html.contains("ci"));
}

#[tokio::test]
async fn render_escapes_what_a_publisher_controls() {
    let mut hostile = package("forge", "1.0.0", false);
    hostile.description = Some("<script>alert(1)</script>".to_string());
    hostile.manifest = Json(serde_json::json!({
        "meta": {"<img src=x onerror=alert(1)>": "<b>bold</b>"}
    }));

    let html = body_html(render_rivets_page(&[hostile], Some("forge"), None, false)).await;
    assert!(!html.contains("<script>alert(1)"), "{html}");
    assert!(!html.contains("<img src=x"), "{html}");
    assert!(!html.contains("<b>bold</b>"), "{html}");
}

#[tokio::test]
async fn render_shows_yank_controls_only_to_a_caller_who_may_manage() {
    let packages = vec![package("forge", "1.0.0", false)];

    let with_manage = body_html(render_rivets_page(
        &packages,
        Some("forge"),
        Some("1.0.0"),
        true,
    ))
    .await;
    assert!(with_manage.contains("ui_rivet_yank"));
    assert!(with_manage.contains("/rivets/yank"));

    let without = body_html(render_rivets_page(
        &packages,
        Some("forge"),
        Some("1.0.0"),
        false,
    ))
    .await;
    assert!(!without.contains("ui_rivet_yank"));
    assert!(!without.contains("/rivets/yank"));
}

#[tokio::test]
async fn render_offers_unyank_for_a_yanked_version() {
    let packages = vec![package("forge", "1.0.0", true)];
    let html = body_html(render_rivets_page(
        &packages,
        Some("forge"),
        Some("1.0.0"),
        true,
    ))
    .await;
    assert!(html.contains("ui_rivet_unyank"));
    assert!(html.contains("/rivets/unyank"));
}

// -----------------------------------------------------------------
// HTTP handlers - the rivet flag is read per call, so unlike the artifact
// page these can be driven through their enabled paths too.
// -----------------------------------------------------------------

async fn app(jwt: JwtConfig, db: Db) -> (Arc<dyn Endpoint>, Arc<quench_http::di::Container>) {
    warehouse_service::routers::ui::register_routes();
    let container = support::container_builder()
        .provide(jwt)
        .provide(db)
        .build()
        .await
        .unwrap();
    support::app(container).await
}

async fn db_with(packages: &[RivetPackage]) -> Db {
    let db = Db::InMemory(InMemoryDb::new());
    for package in packages {
        db.repository::<RivetPackage>()
            .create(package)
            .await
            .unwrap();
    }
    db
}

#[tokio::test]
async fn the_catalog_redirects_to_login_without_a_session() {
    let _rivets = WithRivets::new();
    let (app, container) = app(jwt_config(true), db_with(&[]).await).await;
    let resp = app
        .call(support::req(Method::GET, "/ui/rivets/catalog", &container))
        .await;
    assert!(resp.status().is_redirection());
}

#[tokio::test]
async fn the_catalog_lists_what_the_database_holds() {
    let _rivets = WithRivets::new();
    let db = db_with(&[
        package("forge", "1.0.0", false),
        package("vllm", "2.0.0", false),
    ])
    .await;
    let (app, container) = app(jwt_config(false), db).await;

    let resp = app
        .call(support::req(
            Method::GET,
            "/ui/rivets/catalog?repo=forge&tag=1.0.0",
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let html = body_html(resp).await;
    assert!(html.contains("vllm"));
    assert!(html.contains("riveter install forge@1.0.0"));

    // The trailing-slash form is the same page.
    let resp = app
        .call(support::req(Method::GET, "/ui/rivets/catalog/", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn the_catalog_is_empty_when_the_registry_is_disabled() {
    let _rivets = WithRivets::disabled();
    let db = db_with(&[package("hidden-pkg", "1.0.0", false)]).await;
    let (app, container) = app(jwt_config(false), db).await;

    let resp = app
        .call(support::req(Method::GET, "/ui/rivets/catalog", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let html = body_html(resp).await;
    assert!(html.contains("ui_rivet_empty"));
    assert!(!html.contains("hidden-pkg"));
}

#[tokio::test]
async fn the_root_redirects_to_the_catalog() {
    let _rivets = WithRivets::new();
    let (app, container) = app(jwt_config(false), db_with(&[]).await).await;

    for path in ["/ui/rivets", "/ui/rivets/"] {
        let resp = app.call(support::req(Method::GET, path, &container)).await;
        assert_eq!(resp.status(), StatusCode::PERMANENT_REDIRECT, "{path}");
        let (parts, _) = resp.into_hyper().into_parts();
        assert!(
            parts.headers["location"]
                .to_str()
                .unwrap()
                .ends_with("/ui/rivets/catalog"),
            "{path}"
        );
    }
}

#[tokio::test]
async fn yank_and_unyank_flip_the_flag_and_send_the_page_back_to_the_version() {
    let _rivets = WithRivets::new();
    let db = db_with(&[package("forge", "0.4.0+b1", false)]).await;
    let (app, container) = app(jwt_config(false), db.clone()).await;

    let resp = app
        .call(support::form_req(
            Method::POST,
            "/ui/rivets/yank",
            &[("name", "forge"), ("version", "0.4.0+b1")],
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    let (parts, _) = resp.into_hyper().into_parts();
    let redirect = parts.headers["hx-redirect"].to_str().unwrap().to_string();
    // `+` must not travel raw in a query string, where it means a space.
    assert!(
        redirect.ends_with("/ui/rivets/catalog?repo=forge&tag=0.4.0%2Bb1"),
        "{redirect}"
    );

    let row = db
        .repository::<RivetPackage>()
        .read("forge@0.4.0+b1")
        .await
        .unwrap()
        .unwrap();
    assert!(row.yanked);

    let resp = app
        .call(support::form_req(
            Method::POST,
            "/ui/rivets/unyank",
            &[("name", "forge"), ("version", "0.4.0+b1")],
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    let row = db
        .repository::<RivetPackage>()
        .read("forge@0.4.0+b1")
        .await
        .unwrap()
        .unwrap();
    assert!(!row.yanked);
}

#[tokio::test]
async fn yanking_something_that_does_not_exist_is_not_found() {
    let _rivets = WithRivets::new();
    let (app, container) = app(jwt_config(false), db_with(&[]).await).await;

    let resp = app
        .call(support::form_req(
            Method::POST,
            "/ui/rivets/yank",
            &[("name", "ghost"), ("version", "1.0.0")],
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn yank_and_unyank_redirect_to_login_without_a_session() {
    let _rivets = WithRivets::new();
    let (app, container) = app(jwt_config(true), db_with(&[]).await).await;

    for path in ["/ui/rivets/yank", "/ui/rivets/unyank"] {
        let resp = app
            .call(support::form_req(
                Method::POST,
                path,
                &[("name", "forge"), ("version", "1.0.0")],
                &container,
            ))
            .await;
        assert!(resp.status().is_redirection(), "{path}");
    }
}

#[tokio::test]
async fn yank_and_unyank_are_not_found_when_the_registry_is_disabled() {
    let _rivets = WithRivets::disabled();
    let db = db_with(&[package("forge", "1.0.0", false)]).await;
    let (app, container) = app(jwt_config(false), db.clone()).await;

    for path in ["/ui/rivets/yank", "/ui/rivets/unyank"] {
        let resp = app
            .call(support::form_req(
                Method::POST,
                path,
                &[("name", "forge"), ("version", "1.0.0")],
                &container,
            ))
            .await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND, "{path}");
    }
    let row = db
        .repository::<RivetPackage>()
        .read("forge@1.0.0")
        .await
        .unwrap()
        .unwrap();
    assert!(!row.yanked);
}

// -----------------------------------------------------------------
// Home card
// -----------------------------------------------------------------

#[tokio::test]
async fn the_home_page_links_to_the_rivet_catalog_only_when_enabled() {
    {
        let _rivets = WithRivets::new();
        let html = body_html(render_home_page()).await;
        assert!(html.contains("ui_service_rivets_title"));
        assert!(html.contains("/rivets/catalog"));
    }
    {
        let _rivets = WithRivets::disabled();
        let html = body_html(render_home_page()).await;
        assert!(!html.contains("ui_service_rivets_title"));
    }
}
