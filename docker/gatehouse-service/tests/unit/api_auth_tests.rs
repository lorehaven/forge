use gatehouse_service::api::auth::*;
use quench_auth::prelude::{JwtConfig, Permissions, Role, User};

fn config() -> JwtConfig {
    let mut config = JwtConfig::for_tests();
    config.service_name = "gatehouse".to_string();
    config.audiences = vec![
        "sage".to_string(),
        "switchboard".to_string(),
        "warehouse".to_string(),
    ];
    config
}

fn user(roles: Vec<Role>, grants: &[(&str, &[&str])]) -> User {
    let permissions: Permissions = grants
        .iter()
        .map(|(service, actions)| {
            (
                (*service).to_string(),
                actions.iter().map(|action| action.to_string()).collect(),
            )
        })
        .collect();
    User::new(
        "someone".to_string(),
        "password".to_string(),
        roles,
        permissions,
        None,
    )
    .unwrap()
}

#[test]
fn a_grant_becomes_a_scope_entry_per_action() {
    let scope = user_scope(&user(
        vec![Role::User],
        &[("sage", &["write"]), ("warehouse", &["read"])],
    ));

    assert_eq!(scope, "user sage:write warehouse:read");
}

/// Two actions on the same service become two tokens, not a combined one.
#[test]
fn several_actions_on_one_service_become_several_scope_entries() {
    let scope = user_scope(&user(
        vec![Role::User],
        &[("switchboard", &["read", "launch"])],
    ));

    assert_eq!(scope, "user switchboard:launch switchboard:read");
}

/// The token stays small and stays true as the estate grows.
#[test]
fn a_wildcard_role_emits_the_role_and_nothing_else() {
    let scope = user_scope(&user(vec![Role::Admin], &[("sage", &["read"])]));
    assert_eq!(scope, "admin");
}

#[test]
fn audiences_narrow_to_the_services_a_user_was_granted() {
    let config = config();
    let audiences = user_audiences(&config, &user(vec![Role::User], &[("sage", &["read"])]));

    assert!(audiences.contains(&"sage".to_string()));
    assert!(!audiences.contains(&"switchboard".to_string()));
    assert!(!audiences.contains(&"warehouse".to_string()));
}

#[test]
fn an_admin_keeps_the_whole_realm() {
    let config = config();
    let audiences = user_audiences(&config, &user(vec![Role::Admin], &[]));
    assert_eq!(audiences, config.audiences);
}

/// Gatehouse serves the login page, the home page and refresh. A token that
/// excluded it would leave the holder unable to reach the one service that
/// could grant them anything.
#[test]
fn gatehouse_is_always_an_audience() {
    let config = config();

    for holder in [
        user(vec![Role::User], &[]),
        user(vec![Role::User], &[("sage", &["read"])]),
    ] {
        let audiences = user_audiences(&config, &holder);
        assert!(
            audiences.contains(&"gatehouse".to_string()),
            "gatehouse missing from {audiences:?}"
        );
    }
}

#[test]
fn a_user_with_no_grants_gets_gatehouse_alone() {
    let config = config();
    let audiences = user_audiences(&config, &user(vec![Role::User], &[]));
    assert_eq!(audiences, vec!["gatehouse".to_string()]);
}

/// A grant left behind after a service was removed from the deployment must
/// not put that service back into an audience list.
#[test]
fn a_grant_for_a_service_this_deployment_does_not_run_is_ignored() {
    let config = config();
    let audiences = user_audiences(
        &config,
        &user(vec![Role::User], &[("conveyor", &["write"])]),
    );

    assert_eq!(audiences, vec!["gatehouse".to_string()]);
}
