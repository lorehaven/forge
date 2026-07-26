//! Unit tests for `actix/domain/jwt.rs`.

use quench_auth::actix::domain::jwt::*;

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

/// Tokens minted before the shared realm carry only `service`; they must
/// keep working for the length of one rollout.
#[test]
fn legacy_single_service_tokens_still_verify() {
    let legacy = Claims {
        sub: "user".to_string(),
        aud: vec![],
        service: "switchboard".to_string(),
        scope: "admin".to_string(),
        exp: 0,
        iat: 0,
        sid: None,
    };

    assert!(legacy.allows("switchboard"));
    assert!(!legacy.allows("sage"));
}

#[test]
fn issued_tokens_round_trip_through_the_audience_check() {
    envmnt::set("JWT_SECRET", "test_secret");
    let mut config = JwtConfig::init();
    config.service_name = "gatehouse".to_string();
    config.audiences = vec!["sage".to_string(), "switchboard".to_string()];

    let token = config
        .issue_access_token("user".to_string(), "admin".to_string(), None)
        .unwrap();
    let claims = config.decode_claims(&token).unwrap();

    assert!(claims.allows("sage"));
    assert!(claims.allows("switchboard"));
    assert!(!claims.allows("warehouse"));
    assert_eq!(claims.roles(), vec!["admin".to_string()]);
}

#[test]
fn test_jwt_expiration() {
    envmnt::set("JWT_SECRET", "test_secret");
    let config = JwtConfig::init();

    let claims = Claims::new(
        "user".to_string(),
        "service".to_string(),
        "scope".to_string(),
        None,
        -300,
    ); // Expired 5 minutes ago
    let token = config.encode_claims(&claims).unwrap();

    let result = config.decode_claims(&token);
    assert!(
        result.is_err(),
        "Expired token should be rejected: {:?}",
        result
    );
}

#[test]
fn test_jwt_iat_future() {
    envmnt::set("JWT_SECRET", "test_secret");
    let config = JwtConfig::init();

    let now = chrono::Utc::now();
    let iat = (now + chrono::Duration::seconds(300)).timestamp() as usize; // Issued 5 minutes in the future
    let exp = (now + chrono::Duration::seconds(600)).timestamp() as usize;

    let claims = Claims {
        sub: "user".to_string(),
        aud: vec!["service".to_string()],
        service: "service".to_string(),
        scope: "scope".to_string(),
        exp,
        iat,
        sid: None,
    };

    let token = config.encode_claims(&claims).unwrap();

    let result = config.decode_claims(&token);
    assert!(
        result.is_err(),
        "Token with future iat should be rejected: {:?}",
        result
    );
}
