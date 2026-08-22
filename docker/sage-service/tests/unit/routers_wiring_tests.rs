//! Unit tests for the pure scope-composition functions in `routers/mod.rs`
//! and `routers/ui/mod.rs`. These are straight-line `.service()`/`.wrap()`
//! chains with no branching, so building the `App` once (`init_service`
//! evaluates the whole expression tree, including nested `.service()` calls
//! for every mounted route) exercises every line without needing to send
//! any request.

use actix_web::App;
use actix_web::test as actix_test;
use quench_auth::prelude::JwtConfig;
use sage_service::routers::{base_path_scope, root_scope, ui};

#[actix_web::test]
async fn root_scope_builds() {
    let _app = actix_test::init_service(App::new().service(root_scope())).await;
}

#[actix_web::test]
async fn base_path_scope_builds_with_every_nested_scope() {
    let _app =
        actix_test::init_service(App::new().service(base_path_scope(JwtConfig::for_tests()))).await;
}

#[actix_web::test]
async fn ui_scope_builds_with_every_nested_page_and_wrap() {
    let _app =
        actix_test::init_service(App::new().service(ui::scope(JwtConfig::for_tests()))).await;
}

// ---------------------------------------------------------------------------
// `ui::mod`'s root redirect handlers - reachable through `ui::scope`, need
// only `web::Data<JwtConfig>` (unlike most other pages in this module,
// which also touch `Db`/`SwitchboardClient`/etc.).
// ---------------------------------------------------------------------------

use actix_web::http::StatusCode;
use actix_web::web::Data;

#[actix_web::test]
async fn root_redirects_to_login_when_unauthenticated_and_auth_required() {
    let mut jwt_config = JwtConfig::for_tests();
    jwt_config.auth_enabled = true;
    let app = actix_test::init_service(
        App::new()
            .app_data(Data::new(jwt_config.clone()))
            .service(ui::scope(jwt_config)),
    )
    .await;

    for path in ["/ui", "/ui/"] {
        let req = actix_test::TestRequest::get().uri(path).to_request();
        let resp = actix_test::call_service(&app, req).await;
        assert!(resp.status().is_redirection());
        let location = resp
            .headers()
            .get("Location")
            .expect("redirect has a Location header")
            .to_str()
            .unwrap()
            .to_string();
        assert!(
            location.contains("login"),
            "expected a login redirect, got {location}"
        );
    }
}

#[actix_web::test]
async fn root_redirects_to_home_when_auth_is_disabled() {
    // `JwtConfig::for_tests()` defaults `auth_enabled` to false, so
    // `is_ui_authenticated` treats every request as authenticated.
    let jwt_config = JwtConfig::for_tests();
    let app = actix_test::init_service(
        App::new()
            .app_data(Data::new(jwt_config.clone()))
            .service(ui::scope(jwt_config)),
    )
    .await;

    let req = actix_test::TestRequest::get().uri("/ui").to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    let location = resp
        .headers()
        .get("Location")
        .expect("redirect has a Location header")
        .to_str()
        .unwrap()
        .to_string();
    assert!(
        location.contains("/ui/home"),
        "expected a home redirect, got {location}"
    );
}
