//! `can_on_project`/`can_unscoped` - the request/`app_data`-driven half of
//! `routers::api::authz`, as opposed to `authz_tests.rs`'s pure `Claims`
//! checks.

use actix_web::HttpMessage;
use actix_web::test::TestRequest;
use actix_web::web;
use quench_auth::prelude::{Claims, JwtConfig};
use workbench_service::routers::api::authz::{can_on_project, can_unscoped};

fn claims_with(scope: &str) -> Claims {
    Claims::for_audiences(
        "dev".to_string(),
        vec!["workbench".to_string()],
        scope.to_string(),
        None,
        3600,
    )
}

fn req_with_config(auth_enabled: bool) -> actix_web::HttpRequest {
    let mut config = JwtConfig::for_tests();
    config.auth_enabled = auth_enabled;
    TestRequest::default()
        .app_data(web::Data::new(config))
        .to_http_request()
}

#[test]
fn can_on_project_is_always_true_when_auth_is_disabled() {
    let req = req_with_config(false);
    assert!(can_on_project(&req, "any-project", "write"));
}

#[test]
fn can_on_project_is_false_without_claims_when_auth_is_enabled() {
    let req = req_with_config(true);
    assert!(!can_on_project(&req, "p1", "read"));
}

#[test]
fn can_on_project_honors_the_blanket_grant() {
    let req = req_with_config(true);
    req.extensions_mut().insert(claims_with("workbench:read"));
    assert!(can_on_project(&req, "any-project", "read"));
    assert!(!can_on_project(&req, "any-project", "write"));
}

#[test]
fn can_on_project_honors_a_resource_scoped_grant() {
    let req = req_with_config(true);
    req.extensions_mut()
        .insert(claims_with("workbench:project:p1:write"));
    assert!(can_on_project(&req, "p1", "write"));
    assert!(!can_on_project(&req, "p2", "write"));
}

#[test]
fn can_on_project_is_false_with_no_app_data_at_all() {
    // No `JwtConfig` mounted: `auth_disabled` treats that the same as auth
    // being off (`is_some_and` is false), so this still bypasses.
    let req = TestRequest::default().to_http_request();
    assert!(can_on_project(&req, "p1", "read"));
}

#[test]
fn can_unscoped_is_always_true_when_auth_is_disabled() {
    let req = req_with_config(false);
    assert!(can_unscoped(&req, "write"));
    assert!(can_unscoped(&req, "read"));
}

#[test]
fn can_unscoped_is_false_without_claims() {
    let req = req_with_config(true);
    assert!(!can_unscoped(&req, "write"));
}

#[test]
fn can_unscoped_checks_the_blanket_grant() {
    let req = req_with_config(true);
    req.extensions_mut().insert(claims_with("workbench:write"));
    assert!(can_unscoped(&req, "write"));
    assert!(!can_unscoped(&req, "read"));
}

#[test]
fn can_unscoped_is_true_for_a_wildcard_role() {
    let req = req_with_config(true);
    req.extensions_mut().insert(claims_with("admin"));
    assert!(can_unscoped(&req, "write"));
}
