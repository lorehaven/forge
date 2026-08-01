//! Unit tests for `actix/domain/jwt.rs`.

use quench_auth::actix::domain::jwt::*;

fn claims(scope: &str) -> Claims {
    Claims::for_audiences(
        "user".to_string(),
        vec!["sage".to_string()],
        scope.to_string(),
        None,
        900,
    )
}

#[test]
fn realm_tokens_are_accepted_by_every_listed_audience() {
    let claims = Claims::for_audiences(
        "user".to_string(),
        vec!["sage".to_string(), "warehouse".to_string()],
        "admin".to_string(),
        None,
        900,
    );

    assert!(claims.allows("sage"));
    assert!(claims.allows("warehouse"));
    assert!(!claims.allows("switchboard"));
}

#[tokio::test]
async fn issued_tokens_round_trip_through_the_audience_check() {
    let mut config = JwtConfig::for_tests_with_signing();
    config.service_name = "gatehouse".to_string();
    config.audiences = vec!["sage".to_string(), "switchboard".to_string()];

    let token = config
        .issue_access_token("user".to_string(), "admin".to_string(), None)
        .await
        .unwrap();
    let claims = config.decode_claims(&token).await.unwrap();

    assert!(claims.allows("sage"));
    assert!(claims.allows("switchboard"));
    assert!(!claims.allows("warehouse"));
    assert_eq!(claims.roles(), vec!["admin".to_string()]);
}

/// A service granted several actions carries one token per action - the wire
/// format is a flat list of space-separated `service:action` pairs, folded
/// into a set per service.
#[test]
fn a_scope_carries_roles_and_several_actions_per_service() {
    let claims = claims("user sage:write sage:read warehouse:read");

    assert!(claims.has_role("user"));
    assert!(!claims.has_role("admin"));

    let permissions = claims.permissions();
    assert_eq!(
        permissions.get("sage").cloned().unwrap_or_default(),
        ["read", "write"].map(str::to_string).into()
    );
    assert_eq!(
        permissions.get("warehouse").cloned().unwrap_or_default(),
        ["read"].map(str::to_string).into()
    );
}

/// The whole point of dropping the ordered read/write ladder: two actions on
/// one service are independent, and holding one implies nothing about the
/// other.
#[test]
fn actions_on_one_service_are_independent() {
    let claims = claims("user switchboard:launch");

    assert!(claims.can("switchboard", "launch"));
    assert!(!claims.can("switchboard", "stop"));
    assert!(!claims.can("switchboard", "read"));
}

/// A wildcard role short-circuits, which is why an admin's token enumerates
/// nothing.
#[test]
fn a_wildcard_role_reaches_a_service_the_scope_never_mentions() {
    for scope in ["admin", "service"] {
        let claims = claims(scope);

        assert!(claims.has_wildcard(), "{scope} should be a wildcard");
        assert!(claims.can("anything-at-all", "any-action-at-all"));
        assert!(claims.permissions().is_empty());
    }

    let ordinary = claims("user sage:write");
    assert!(!ordinary.has_wildcard());
    assert!(!ordinary.can("switchboard", "read"));
}

/// The reason `has_role` exists at all. The old check was
/// `scope.contains("admin")`, which a service named `admin` would satisfy.
#[test]
fn a_permission_cannot_be_mistaken_for_the_role_it_is_named_after() {
    let claims = claims("user admin:read");

    assert!(!claims.has_role("admin"));
    assert!(!claims.has_wildcard());
    assert!(claims.can("admin", "read"));
    assert!(!claims.can("admin", "write"));
}

#[test]
fn an_unknown_role_name_is_not_a_wildcard() {
    // `system` used to be accepted by switchboard's substring check; the realm
    // never issued it.
    let claims = claims("system");
    assert!(!claims.has_wildcard());
    assert!(!claims.can("sage", "read"));
}

#[test]
fn narrowing_cannot_widen_a_token_past_the_configured_audiences() {
    let mut config = JwtConfig::for_tests();
    config.audiences = vec!["sage".to_string(), "warehouse".to_string()];

    // A grant naming a service this deployment does not run must not appear.
    let narrowed = config.narrow_audiences(&[
        "sage".to_string(),
        "conveyor".to_string(),
        "not-a-service".to_string(),
    ]);

    assert_eq!(narrowed, vec!["sage".to_string()]);
    assert!(config.narrow_audiences(&[]).is_empty());
}

#[tokio::test]
async fn a_narrowed_token_is_rejected_by_the_audience_it_excludes() {
    let mut config = JwtConfig::for_tests_with_signing();
    config.service_name = "gatehouse".to_string();
    config.audiences = vec!["sage".to_string(), "switchboard".to_string()];

    let token = config
        .issue_access_token_for(
            "user".to_string(),
            vec!["sage".to_string()],
            "user sage:read".to_string(),
            None,
        )
        .await
        .unwrap();
    let claims = config.decode_claims(&token).await.unwrap();

    assert!(claims.allows("sage"));
    assert!(
        !claims.allows("switchboard"),
        "the relying party's own audience check is what enforces service access"
    );
}

#[tokio::test]
async fn test_jwt_expiration() {
    let config = JwtConfig::for_tests_with_signing();

    let claims = Claims::for_audiences(
        "user".to_string(),
        vec!["service".to_string()],
        "scope".to_string(),
        None,
        -300,
    ); // Expired 5 minutes ago
    let token = config.encode_claims(&claims).await.unwrap();

    let result = config.decode_claims(&token).await;
    assert!(
        result.is_err(),
        "Expired token should be rejected: {:?}",
        result
    );
}

#[tokio::test]
async fn test_jwt_iat_future() {
    let config = JwtConfig::for_tests_with_signing();

    let now = chrono::Utc::now();
    let iat = (now + chrono::Duration::seconds(300)).timestamp() as usize; // Issued 5 minutes in the future
    let exp = (now + chrono::Duration::seconds(600)).timestamp() as usize;

    let claims = Claims {
        sub: "user".to_string(),
        aud: vec!["service".to_string()],
        scope: "scope".to_string(),
        exp,
        iat,
        sid: None,
    };

    let token = config.encode_claims(&claims).await.unwrap();

    let result = config.decode_claims(&token).await;
    assert!(
        result.is_err(),
        "Token with future iat should be rejected: {:?}",
        result
    );
}
