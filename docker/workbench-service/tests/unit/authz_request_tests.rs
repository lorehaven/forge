//! `can_on_project`/`can_unscoped` - the `JwtConfig`-driven half of
//! `routers::api::authz`, as opposed to `authz_tests.rs`'s pure `Claims`
//! checks.

use quench_auth::domain::jwt::{Claims, JwtConfig};
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

fn config(auth_enabled: bool) -> JwtConfig {
    let mut config = JwtConfig::for_tests();
    config.auth_enabled = auth_enabled;
    config
}

#[test]
fn can_on_project_is_always_true_when_auth_is_disabled() {
    assert!(can_on_project(None, &config(false), "any-project", "write"));
}

#[test]
fn can_on_project_is_false_without_claims_when_auth_is_enabled() {
    assert!(!can_on_project(None, &config(true), "p1", "read"));
}

#[test]
fn can_on_project_honors_the_blanket_grant() {
    let claims = claims_with("workbench:read");
    let cfg = config(true);
    assert!(can_on_project(Some(&claims), &cfg, "any-project", "read"));
    assert!(!can_on_project(Some(&claims), &cfg, "any-project", "write"));
}

#[test]
fn can_on_project_honors_a_resource_scoped_grant() {
    let claims = claims_with("workbench:project:p1:write");
    let cfg = config(true);
    assert!(can_on_project(Some(&claims), &cfg, "p1", "write"));
    assert!(!can_on_project(Some(&claims), &cfg, "p2", "write"));
}

#[test]
fn can_unscoped_is_always_true_when_auth_is_disabled() {
    let cfg = config(false);
    assert!(can_unscoped(None, &cfg, "write"));
    assert!(can_unscoped(None, &cfg, "read"));
}

#[test]
fn can_unscoped_is_false_without_claims() {
    assert!(!can_unscoped(None, &config(true), "write"));
}

#[test]
fn can_unscoped_checks_the_blanket_grant() {
    let claims = claims_with("workbench:write");
    let cfg = config(true);
    assert!(can_unscoped(Some(&claims), &cfg, "write"));
    assert!(!can_unscoped(Some(&claims), &cfg, "read"));
}

#[test]
fn can_unscoped_is_true_for_a_wildcard_role() {
    let claims = claims_with("admin");
    assert!(can_unscoped(Some(&claims), &config(true), "write"));
}
