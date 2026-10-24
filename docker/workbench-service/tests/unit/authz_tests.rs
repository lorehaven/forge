//! Resource-scoped permission logic, in isolation from any HTTP request or
//! database - see `docker/workbench-service/src/routers/api/authz.rs`.

use quench_auth::prelude::Claims;
use workbench_service::routers::api::authz::{can_on_project_claims, granted_project_ids};

fn claims_with(scope: &str) -> Claims {
    Claims::for_audiences(
        "dev".to_string(),
        vec!["workbench".to_string()],
        scope.to_string(),
        None,
        3600,
    )
}

#[quench_test::test]
fn blanket_grant_covers_every_project() {
    let claims = claims_with("workbench:read");
    assert!(can_on_project_claims(&claims, "any-project", "read"));
    assert!(!can_on_project_claims(&claims, "any-project", "write"));
}

#[quench_test::test]
fn scoped_grant_covers_only_its_project() {
    let claims = claims_with("workbench:project:p1:write");
    assert!(can_on_project_claims(&claims, "p1", "write"));
    assert!(!can_on_project_claims(&claims, "p1", "read"));
    assert!(!can_on_project_claims(&claims, "p2", "write"));
}

#[quench_test::test]
fn granted_project_ids_extracts_only_matching_action() {
    let claims = claims_with(
        "workbench:project:p1:read workbench:project:p2:write workbench:project:p3:read",
    );
    let mut ids = granted_project_ids(&claims, "read");
    ids.sort();
    assert_eq!(ids, vec!["p1".to_string(), "p3".to_string()]);
}

#[quench_test::test]
fn no_grant_means_no_access() {
    let claims = claims_with("");
    assert!(!can_on_project_claims(&claims, "p1", "read"));
}

#[quench_test::test]
fn admin_wildcard_covers_everything() {
    let claims = claims_with("admin");
    assert!(can_on_project_claims(&claims, "p1", "write"));
    assert!(can_on_project_claims(&claims, "anything-at-all", "read"));
}
