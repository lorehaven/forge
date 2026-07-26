//! Unit tests for `actix/domain/realm.rs`.

use quench_auth::actix::domain::realm::*;

/// `envmnt` is process-global, so these assertions share one test to keep
/// them from racing each other.
#[test]
fn realm_names_come_from_the_environment() {
    envmnt::remove("AUTH_DB_SCHEMA");
    envmnt::remove("AUTH_COOKIE_NAME");
    envmnt::remove("GATEHOUSE_URL");

    assert_eq!(auth_schema(), "auth");
    assert_eq!(session_cookie_name(), "forge_session");
    assert_eq!(refresh_cookie_name(), "forge_refresh");
    assert!(gatehouse_login_url(None).is_none());

    envmnt::set("GATEHOUSE_URL", "https://gate.example.com/gatehouse/");
    assert_eq!(
        gatehouse_login_url(None).unwrap(),
        "https://gate.example.com/gatehouse/ui/login"
    );
    assert_eq!(
        gatehouse_login_url(Some("https://sage.example.com/ui/home")).unwrap(),
        "https://gate.example.com/gatehouse/ui/login?redirect=https%3A%2F%2Fsage.example.com%2Fui%2Fhome"
    );
    envmnt::remove("GATEHOUSE_URL");
}
