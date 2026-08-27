//! The management-permission predicate for the warehouse UI, in isolation -
//! see `docker/warehouse-service/src/routers/ui/authz.rs`. It has to match
//! `routers::files::authz::has_blanket("write")` and the APK scope's
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

#[test]
fn an_admin_wildcard_may_manage() {
    assert!(can_manage(&claims("admin")));
}

#[test]
fn a_service_wildcard_may_manage() {
    assert!(can_manage(&claims("service")));
}

#[test]
fn the_blanket_warehouse_write_grant_may_manage() {
    assert!(can_manage(&claims("warehouse:read warehouse:write")));
}

#[test]
fn a_read_only_grant_may_not_manage() {
    assert!(!can_manage(&claims("warehouse:read")));
}

#[test]
fn no_grants_at_all_may_not_manage() {
    assert!(!can_manage(&claims("")));
}

#[test]
fn write_on_a_different_service_does_not_carry_over() {
    assert!(!can_manage(&claims("conveyor:write")));
}

#[test]
fn a_storage_scoped_grant_is_not_blanket_write() {
    // `warehouse:storage:<name>:write` unlocks one storage's contents, not
    // storage administration - provisioning stays admin/editor only.
    assert!(!can_manage(&claims("warehouse:storage:backups:write")));
}
