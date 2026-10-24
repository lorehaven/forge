//! The management-permission predicate for the warehouse UI, in isolation -
//! see `docker/warehouse-service/src/routers/ui/authz.rs`. It has to match
//! `routers::files::authz::has_blanket("write")` and the artifact scope's
//! `RequireWrite`: a wildcard role, or the blanket `warehouse:write` grant.

use quench_auth::prelude::Claims;
use warehouse_service::routers::ui::authz::can_manage;

fn claims(scope: &str) -> Claims {
    Claims::for_audiences(
        "user".to_string(),
        vec!["warehouse".to_string()],
        scope.to_string(),
        None,
        3600,
    )
}

#[quench_test::test]
fn an_admin_wildcard_may_manage() {
    assert!(can_manage(&claims("admin")));
}

#[quench_test::test]
fn a_service_wildcard_may_manage() {
    assert!(can_manage(&claims("service")));
}

#[quench_test::test]
fn the_blanket_warehouse_write_grant_may_manage() {
    assert!(can_manage(&claims("warehouse:read warehouse:write")));
}

#[quench_test::test]
fn a_read_only_grant_may_not_manage() {
    assert!(!can_manage(&claims("warehouse:read")));
}

#[quench_test::test]
fn no_grants_at_all_may_not_manage() {
    assert!(!can_manage(&claims("")));
}

#[quench_test::test]
fn write_on_a_different_service_does_not_carry_over() {
    assert!(!can_manage(&claims("conveyor:write")));
}

#[quench_test::test]
fn a_storage_scoped_grant_is_not_blanket_write() {
    // `warehouse:storage:<name>:write` unlocks one storage's contents, not
    // storage administration - provisioning stays admin/editor only.
    assert!(!can_manage(&claims("warehouse:storage:backups:write")));
}
