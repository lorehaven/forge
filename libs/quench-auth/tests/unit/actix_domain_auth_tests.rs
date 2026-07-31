//! Unit tests for `actix/domain/auth.rs`.

use quench_auth::actix::domain::auth::*;

fn grants(pairs: &[(&str, &[&str])]) -> Permissions {
    pairs
        .iter()
        .map(|(service, actions)| {
            (
                (*service).to_string(),
                actions.iter().map(|action| action.to_string()).collect(),
            )
        })
        .collect()
}

fn user(roles: Vec<Role>, permissions: &[(&str, &[&str])]) -> User {
    User::new(
        "someone".into(),
        "password".into(),
        roles,
        grants(permissions),
        None,
    )
    .unwrap()
}

#[test]
fn passwords_use_argon2id_with_unique_salts() {
    let first = User::new(
        "first".into(),
        "password".into(),
        vec![Role::User],
        Permissions::new(),
        None,
    )
    .unwrap();
    let second = User::new(
        "second".into(),
        "password".into(),
        vec![Role::User],
        Permissions::new(),
        None,
    )
    .unwrap();

    assert!(first.password.starts_with("$argon2id$"));
    assert_ne!(first.password, second.password);
    assert!(first.verify_password("password"));
    assert!(!first.verify_password("wrong"));
}

#[test]
fn an_ordinary_user_holds_only_what_is_granted() {
    let user = user(vec![Role::User], &[("sage", &["read"])]);

    assert!(user.can("sage", "read"));
    assert!(!user.can("sage", "write"));
    assert!(!user.can("warehouse", "read"));
}

/// The whole point of a flat action set rather than an ordered level: two
/// unrelated actions on the same service are independently grantable, and
/// holding one says nothing about the other.
#[test]
fn actions_on_one_service_are_independent() {
    let user = user(vec![Role::User], &[("switchboard", &["launch"])]);

    assert!(user.can("switchboard", "launch"));
    assert!(!user.can("switchboard", "stop"));
    assert!(!user.can("switchboard", "delete-model"));
}

/// The wildcard is the whole reason adding a service to the estate does not mean
/// re-granting anything to administrators.
#[test]
fn wildcard_roles_reach_a_service_with_no_grant_written_down() {
    for role in [Role::Admin, Role::Service] {
        let holder = user(vec![role.clone()], &[]);

        assert!(holder.has_wildcard(), "{role} should be a wildcard");
        assert!(holder.can("a-service-invented-just-now", "anything-at-all"));
        assert!(
            holder.get_permissions().is_empty(),
            "a wildcard should not need enumerated grants"
        );
    }
}

#[test]
fn the_plain_user_role_is_not_a_wildcard() {
    assert!(!Role::User.is_wildcard());
    assert!(!user(vec![Role::User], &[]).can("sage", "read"));
}

/// A row that will not parse as `{service: [action, ...]}` reads as no grants
/// at all, not a partial one - "no access" is the safe direction for that
/// failure to go.
#[test]
fn an_unreadable_permissions_column_reads_as_no_grants() {
    let mut user = user(vec![Role::User], &[("sage", &["read"])]);
    user.permissions = serde_json::json!("not an object");

    assert!(user.get_permissions().is_empty());
    assert!(!user.can("sage", "read"));
}

/// Omit this from `columns` and CRUD silently drops the column on every write.
#[test]
fn permissions_is_a_persisted_column() {
    use quench_db::prelude::Model;
    assert!(User::columns().contains(&"permissions"));
}
