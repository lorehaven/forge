//! What a token lets the caller do: the blanket and the per-package grants, and the dev bypass.

use gantry_service::routers::api::authz::{can, can_on_target, granted_targets};
use quench_auth::domain::jwt::{Claims, JwtConfig};

fn claims_with(scope: &str) -> Claims {
    Claims::for_audiences(
        "dev".to_string(),
        vec!["gantry".to_string()],
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
fn everything_is_allowed_when_auth_is_off_and_nothing_without_a_token_when_it_is_on() {
    assert!(can(None, &config(false), "deploy"));
    assert!(can_on_target(None, &config(false), "media", "deploy"));

    assert!(!can(None, &config(true), "read"));
    assert!(!can_on_target(None, &config(true), "media", "read"));
}

#[test]
fn a_blanket_grant_covers_every_package_but_only_its_action() {
    let claims = claims_with("gantry:read");
    let cfg = config(true);
    assert!(can(Some(&claims), &cfg, "read"));
    assert!(can_on_target(Some(&claims), &cfg, "media", "read"));
    assert!(
        !can(Some(&claims), &cfg, "deploy"),
        "read does not imply deploy"
    );
    assert!(!can_on_target(Some(&claims), &cfg, "media", "deploy"));
}

#[test]
fn a_package_grant_covers_that_package_and_that_action_only() {
    let claims = claims_with("gantry:target:media:deploy");
    let cfg = config(true);
    assert!(can_on_target(Some(&claims), &cfg, "media", "deploy"));
    assert!(!can_on_target(Some(&claims), &cfg, "media", "scale"));
    assert!(!can_on_target(Some(&claims), &cfg, "forge", "deploy"));
    // A package grant is not the blanket one.
    assert!(!can(Some(&claims), &cfg, "deploy"));
}

#[test]
fn the_admin_wildcard_covers_everything() {
    let claims = claims_with("admin");
    let cfg = config(true);
    assert!(can(Some(&claims), &cfg, "activate"));
    assert!(can_on_target(Some(&claims), &cfg, "anything", "rollback"));
}

#[test]
fn another_services_grant_means_nothing_here() {
    let claims = claims_with("workbench:write warehouse:read");
    assert!(!can(Some(&claims), &config(true), "read"));
}

#[test]
fn granted_targets_lists_only_the_packages_granted_that_action() {
    let claims = claims_with(
        "gantry:target:media:deploy gantry:target:vpn:scale gantry:target:forge:deploy gantry:read",
    );
    let mut targets = granted_targets(&claims, "deploy");
    targets.sort();
    assert_eq!(targets, ["forge", "media"]);
    assert_eq!(granted_targets(&claims, "scale"), ["vpn"]);
    assert!(granted_targets(&claims, "rollback").is_empty());
}
