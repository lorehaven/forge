//! `load_paths`, `is_admin`, and `can` from `routers/models/mod_impl.rs`.
//!
//! `is_admin`/`can` are pure functions over `Option<&Claims>` now (the
//! `OptionalClaims` extractor is what reads them out of the request/cookie -
//! see `routers_models_handlers_tests.rs` for that side), so these tests
//! call them directly with constructed `Claims` values instead of building
//! requests.

use quench_auth::domain::jwt::{Claims, JwtConfig};
use switchboard_service::routers::models::mod_impl::{can, is_admin, load_paths};

fn claims_with_scope(scope: &str) -> Claims {
    Claims::for_audiences(
        "user-1".to_string(),
        vec!["switchboard".to_string()],
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
fn is_admin_is_always_true_when_auth_is_disabled() {
    assert!(is_admin(None, &config(false)));
}

#[test]
fn is_admin_is_false_without_claims() {
    assert!(!is_admin(None, &config(true)));
}

#[test]
fn is_admin_is_true_for_a_wildcard_role() {
    let claims = claims_with_scope("admin");
    assert!(is_admin(Some(&claims), &config(true)));
}

#[test]
fn is_admin_is_false_for_a_non_wildcard_role() {
    let claims = claims_with_scope("switchboard:read");
    assert!(!is_admin(Some(&claims), &config(true)));
}

#[test]
fn can_is_always_true_when_auth_is_disabled() {
    assert!(can(None, &config(false), "launch"));
}

#[test]
fn can_checks_the_specific_action_against_the_service_name() {
    let cfg = config(true);
    let claims = claims_with_scope(&format!("{}:launch", cfg.service_name));

    assert!(can(Some(&claims), &cfg, "launch"));
    assert!(!can(Some(&claims), &cfg, "stop"));
}

#[test]
fn can_is_false_without_claims() {
    assert!(!can(None, &config(true), "launch"));
}

#[test]
fn load_paths_splits_the_env_value_on_colons_and_trims_entries() {
    let key = "SWITCHBOARD_TEST_LOAD_PATHS_A";
    unsafe { std::env::set_var(key, " /a/one : /a/two ::") };
    assert_eq!(
        load_paths(key, &["/default"]),
        vec!["/a/one".to_string(), "/a/two".to_string()]
    );
    unsafe { std::env::remove_var(key) };
}

#[test]
fn load_paths_falls_back_to_defaults_when_env_is_unset_or_blank() {
    let key = "SWITCHBOARD_TEST_LOAD_PATHS_B";
    unsafe { std::env::remove_var(key) };
    assert_eq!(
        load_paths(key, &["/default/a", "/default/b"]),
        vec!["/default/a", "/default/b"]
    );

    unsafe { std::env::set_var(key, "   ") };
    assert_eq!(load_paths(key, &["/default/a"]), vec!["/default/a"]);
    unsafe { std::env::remove_var(key) };
}
