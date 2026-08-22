//! `load_paths`, `is_admin`, and `can` from `routers/models/mod_impl.rs`.
//!
//! `is_admin`/`can` read `Claims` out of request extensions (the same place
//! `Auth` middleware puts them), so these tests build requests directly
//! rather than exercising the middleware itself.

use crate::env_support::env_lock;
use actix_web::HttpMessage;
use actix_web::test::TestRequest;
use actix_web::web;
use quench_auth::actix::domain::jwt::Claims;
use quench_auth::prelude::JwtConfig;
use switchboard_service::routers::models::mod_impl::{can, is_admin, load_paths};

fn claims_with_scope(scope: &str) -> Claims {
    Claims::for_audiences(
        "user-1".to_string(),
        vec!["switchboard".to_string()],
        scope.to_string(),
        None,
        3600,
    )
}

#[tokio::test]
async fn is_admin_is_always_true_when_auth_is_disabled() {
    let _guard = env_lock().lock().await;
    unsafe { std::env::set_var("SERVICE_AUTH_ENABLED", "false") };
    let config = web::Data::new(JwtConfig::for_tests());

    let req = TestRequest::default().to_http_request();
    assert!(is_admin(&req, &config).await);
    unsafe { std::env::remove_var("SERVICE_AUTH_ENABLED") };
}

#[tokio::test]
async fn is_admin_is_false_without_claims_or_a_session_cookie() {
    let _guard = env_lock().lock().await;
    unsafe { std::env::set_var("SERVICE_AUTH_ENABLED", "true") };
    let config = web::Data::new(JwtConfig::for_tests());

    let req = TestRequest::default().to_http_request();
    assert!(!is_admin(&req, &config).await);

    unsafe { std::env::remove_var("SERVICE_AUTH_ENABLED") };
}

#[tokio::test]
async fn is_admin_is_true_for_a_wildcard_role_in_request_extensions() {
    let _guard = env_lock().lock().await;
    unsafe { std::env::set_var("SERVICE_AUTH_ENABLED", "true") };
    let config = web::Data::new(JwtConfig::for_tests());

    let req = TestRequest::default().to_http_request();
    req.extensions_mut().insert(claims_with_scope("admin"));

    assert!(is_admin(&req, &config).await);
    unsafe { std::env::remove_var("SERVICE_AUTH_ENABLED") };
}

#[tokio::test]
async fn is_admin_is_false_for_a_non_wildcard_role() {
    let _guard = env_lock().lock().await;
    unsafe { std::env::set_var("SERVICE_AUTH_ENABLED", "true") };
    let config = web::Data::new(JwtConfig::for_tests());

    let req = TestRequest::default().to_http_request();
    req.extensions_mut()
        .insert(claims_with_scope("switchboard:read"));

    assert!(!is_admin(&req, &config).await);
    unsafe { std::env::remove_var("SERVICE_AUTH_ENABLED") };
}

#[tokio::test]
async fn can_is_always_true_when_auth_is_disabled() {
    let _guard = env_lock().lock().await;
    unsafe { std::env::set_var("SERVICE_AUTH_ENABLED", "false") };
    let config = web::Data::new(JwtConfig::for_tests());

    let req = TestRequest::default().to_http_request();
    assert!(can(&req, &config, "launch").await);
    unsafe { std::env::remove_var("SERVICE_AUTH_ENABLED") };
}

#[tokio::test]
async fn can_checks_the_specific_action_against_the_service_name() {
    let _guard = env_lock().lock().await;
    unsafe { std::env::set_var("SERVICE_AUTH_ENABLED", "true") };
    let config = web::Data::new(JwtConfig::for_tests());

    let req = TestRequest::default().to_http_request();
    req.extensions_mut().insert(claims_with_scope(&format!(
        "{}:launch",
        config.service_name
    )));

    assert!(can(&req, &config, "launch").await);
    assert!(!can(&req, &config, "stop").await);
    unsafe { std::env::remove_var("SERVICE_AUTH_ENABLED") };
}

#[tokio::test]
async fn can_is_false_without_claims_or_a_session_cookie() {
    let _guard = env_lock().lock().await;
    unsafe { std::env::set_var("SERVICE_AUTH_ENABLED", "true") };
    let config = web::Data::new(JwtConfig::for_tests());

    let req = TestRequest::default().to_http_request();
    assert!(!can(&req, &config, "launch").await);
    unsafe { std::env::remove_var("SERVICE_AUTH_ENABLED") };
}

#[test]
fn load_paths_splits_the_env_value_on_colons_and_trims_entries() {
    let key = "SWITCHBOARD_TEST_LOAD_PATHS_A";
    unsafe { std::env::set_var(key, " /a/one : /a/two ::") };
    assert_eq!(
        load_paths(key, &["/default"]),
        vec!["/a/one".to_string(), "/a/two".to_string()]
    );
    unsafe { std::env::remove_var(key) };
}

#[test]
fn load_paths_falls_back_to_defaults_when_env_is_unset_or_blank() {
    let key = "SWITCHBOARD_TEST_LOAD_PATHS_B";
    unsafe { std::env::remove_var(key) };
    assert_eq!(
        load_paths(key, &["/default/a", "/default/b"]),
        vec!["/default/a", "/default/b"]
    );

    unsafe { std::env::set_var(key, "   ") };
    assert_eq!(load_paths(key, &["/default/a"]), vec!["/default/a"]);
    unsafe { std::env::remove_var(key) };
}
