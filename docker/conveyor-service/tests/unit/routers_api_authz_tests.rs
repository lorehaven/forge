//! `routers::api::authz` - the resource-scoped access checks on top of the
//! blanket `conveyor:write`/`conveyor:read` grant.

use actix_web::HttpMessage;
use actix_web::test::TestRequest;
use actix_web::web;
use conveyor_service::routers::api::authz::{
    can_on_project_claims, can_unscoped, granted_project_ids,
};
use quench_auth::prelude::{Claims, JwtConfig};
use quench_db::prelude::Db;

fn claims_with_scope(scope: &str) -> Claims {
    Claims::for_audiences(
        "user-1".to_string(),
        vec!["conveyor".to_string()],
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
    req.extensions_mut()
        .insert(claims_with_scope("conveyor:write"));
    assert!(can_unscoped(&req, "write"));
    assert!(!can_unscoped(&req, "read"));
}

#[test]
fn can_unscoped_is_true_for_a_wildcard_role() {
    let req = req_with_config(true);
    req.extensions_mut().insert(claims_with_scope("admin"));
    assert!(can_unscoped(&req, "write"));
}

#[test]
fn granted_project_ids_extracts_resource_scoped_grants_for_the_action() {
    let claims = claims_with_scope(
        "conveyor:project:proj-a:write conveyor:project:proj-b:read conveyor:project:proj-c:write",
    );
    let mut ids = granted_project_ids(&claims, "write");
    ids.sort();
    assert_eq!(ids, vec!["proj-a".to_string(), "proj-c".to_string()]);
}

#[test]
fn granted_project_ids_is_empty_without_any_matching_grant() {
    let claims = claims_with_scope("conveyor:read sage:write");
    assert!(granted_project_ids(&claims, "write").is_empty());
}

#[test]
fn granted_project_ids_ignores_malformed_entries() {
    // No trailing `:action` to split on.
    let claims = claims_with_scope("conveyor:project:proj-a");
    assert!(granted_project_ids(&claims, "write").is_empty());
}

#[tokio::test]
async fn can_on_project_claims_short_circuits_on_the_blanket_grant_without_touching_the_db() {
    // In-memory `Db` errors on the ancestor-chain query, but the blanket
    // grant check happens first and never reaches it.
    let db = Db::connect("").await.expect("in-memory database");
    let claims = claims_with_scope("conveyor:write");
    assert!(can_on_project_claims(&claims, &db, "any-project", "write").await);
}

#[tokio::test]
async fn can_on_project_claims_is_false_without_a_matching_grant_when_the_chain_lookup_fails() {
    // The ancestor-chain query fails against an in-memory `Db`, which
    // `can_on_project_claims` treats the same as "no ancestors" (`unwrap_or_default`)
    // rather than propagating the error - so a caller with no blanket grant
    // and an unresolvable chain is simply denied, not error'd.
    let db = Db::connect("").await.expect("in-memory database");
    let claims = claims_with_scope("conveyor:read");
    assert!(!can_on_project_claims(&claims, &db, "some-project", "write").await);
}
