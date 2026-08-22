//! Unit tests for `routers/ui/mod.rs`'s root redirects.
//!
//! `is_ui_authenticated`'s `config.auth_enabled` is a plain field on the
//! `JwtConfig` instance each test constructs, so these mutate that directly
//! rather than the `SERVICE_AUTH_ENABLED` env var - no cross-test race to
//! guard against, since nothing here is process-global.

use actix_web::http::StatusCode;
use actix_web::web::Data;
use actix_web::{App, test as actix_test};
use conveyor_service::routers::ui;
use quench_auth::prelude::JwtConfig;

#[actix_web::test]
async fn root_redirects_to_login_when_there_is_no_session_and_auth_is_enabled() {
    let mut config = JwtConfig::for_tests();
    config.auth_enabled = true;

    let app = actix_test::init_service(
        App::new()
            .app_data(Data::new(config.clone()))
            .service(ui::scope(config)),
    )
    .await;

    let req = actix_test::TestRequest::get().uri("/ui").to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    let location = resp
        .headers()
        .get("Location")
        .expect("redirect location")
        .to_str()
        .unwrap();
    assert!(location.contains("login"), "{location}");
}

#[actix_web::test]
async fn root_slash_redirects_home_when_auth_is_disabled() {
    let config = JwtConfig::for_tests();
    assert!(!config.auth_enabled, "for_tests should default auth off");

    let app = actix_test::init_service(
        App::new()
            .app_data(Data::new(config.clone()))
            .service(ui::scope(config)),
    )
    .await;

    let req = actix_test::TestRequest::get().uri("/ui/").to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    let location = resp
        .headers()
        .get("Location")
        .expect("redirect location")
        .to_str()
        .unwrap();
    assert!(location.ends_with("/ui/home"), "{location}");
}
