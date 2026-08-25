//! Resource-scoped permission logic for dynamic storages, in isolation from
//! any HTTP request or database - see
//! `docker/warehouse-service/src/routers/files/authz.rs`.

use chrono::Utc;
use quench_auth::prelude::Claims;
use warehouse_service::domain::storage::DynamicStorage;
use warehouse_service::routers::files::authz::can_on_storage_claims;

fn claims_with(sub: &str, scope: &str) -> Claims {
    Claims::for_audiences(
        sub.to_string(),
        vec!["warehouse".to_string()],
        scope.to_string(),
        None,
        3600,
    )
}

fn storage_owned_by(owner: &str) -> DynamicStorage {
    DynamicStorage {
        name: "backups".to_string(),
        owner: owner.to_string(),
        max_file_bytes: None,
        quota_bytes: 1024,
        used_bytes: 0,
        sync_enabled: false,
        created_at: Utc::now(),
    }
}

#[test]
fn the_owner_has_full_access_with_no_grant_at_all() {
    let claims = claims_with("alice", "");
    let storage = storage_owned_by("alice");
    assert!(can_on_storage_claims(&claims, &storage, "read"));
    assert!(can_on_storage_claims(&claims, &storage, "write"));
}

#[test]
fn a_non_owner_with_no_grant_has_no_access() {
    let claims = claims_with("mallory", "");
    let storage = storage_owned_by("alice");
    assert!(!can_on_storage_claims(&claims, &storage, "read"));
    assert!(!can_on_storage_claims(&claims, &storage, "write"));
}

/// The one deliberate divergence from conveyor/workbench's resource-scoped
/// grants: a blanket `warehouse:read`/`write` grant does **not** unlock a
/// dynamic storage on its own, because private-by-default is the point.
#[test]
fn the_blanket_warehouse_grant_does_not_unlock_a_dynamic_storage() {
    let claims = claims_with("mallory", "warehouse:read warehouse:write");
    let storage = storage_owned_by("alice");
    assert!(!can_on_storage_claims(&claims, &storage, "read"));
    assert!(!can_on_storage_claims(&claims, &storage, "write"));
}

#[test]
fn a_scoped_grant_covers_only_its_own_storage_and_action() {
    let claims = claims_with("bob", "warehouse:storage:backups:read");
    let storage = storage_owned_by("alice");
    let other = storage_owned_by("alice");
    let mut other = other;
    other.name = "other-storage".to_string();

    assert!(can_on_storage_claims(&claims, &storage, "read"));
    assert!(!can_on_storage_claims(&claims, &storage, "write"));
    assert!(!can_on_storage_claims(&claims, &other, "read"));
}

#[test]
fn a_wildcard_role_covers_every_storage() {
    let claims = claims_with("root", "admin");
    let storage = storage_owned_by("alice");
    assert!(can_on_storage_claims(&claims, &storage, "read"));
    assert!(can_on_storage_claims(&claims, &storage, "write"));
}
