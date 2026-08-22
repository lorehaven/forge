use crate::support;

use warehouse_service::docker_token::{DockerClaims, DockerTokenConfig};

fn config() -> DockerTokenConfig {
    DockerTokenConfig {
        secret: b"test-secret-at-least-this-long".to_vec(),
        service_name: "warehouse".to_string(),
        realm: "https://warehouse.test/token".to_string(),
        auth_enabled: true,
    }
}

fn claims(exp: usize) -> DockerClaims {
    DockerClaims {
        sub: "alice".to_string(),
        service: "warehouse".to_string(),
        scope: "repository:my/repo:pull,push".to_string(),
        iat: 0,
        exp,
    }
}

#[test]
fn encode_then_decode_round_trips_every_field() {
    let config = config();
    let original = claims(usize::MAX / 2);

    let token = config.encode(&original).expect("encode");
    let decoded = config.decode(&token).expect("decode");

    assert_eq!(decoded.sub, original.sub);
    assert_eq!(decoded.service, original.service);
    assert_eq!(decoded.scope, original.scope);
    assert_eq!(decoded.exp, original.exp);
    assert_eq!(decoded.iat, original.iat);
}

#[test]
fn decode_rejects_an_expired_token() {
    let config = config();
    let token = config.encode(&claims(1)).expect("encode");

    let error = config.decode(&token).unwrap_err();
    assert_eq!(
        error.kind(),
        &jsonwebtoken::errors::ErrorKind::ExpiredSignature
    );
}

#[test]
fn decode_rejects_a_token_signed_with_a_different_secret() {
    let signer = config();
    let token = signer.encode(&claims(usize::MAX / 2)).expect("encode");

    let mut verifier = config();
    verifier.secret = b"a-completely-different-secret".to_vec();

    assert!(verifier.decode(&token).is_err());
}

#[test]
fn decode_rejects_garbage_input() {
    assert!(config().decode("not.a.token").is_err());
}

#[test]
fn init_builds_a_working_config_from_the_docker_token_secret_env_var() {
    // `middleware::auth`'s tests also construct configs via `init`, so this
    // holds `secret_env_lock` for as long as `DOCKER_TOKEN_SECRET` is set.
    let _guard = support::secret_env_lock()
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    unsafe { std::env::set_var("DOCKER_TOKEN_SECRET", "unit-test-secret-value") };

    let config = DockerTokenConfig::init("warehouse".to_string(), "r".to_string(), false);
    assert!(!config.auth_enabled);
    assert_eq!(config.service_name, "warehouse");

    let token = config.encode(&claims(usize::MAX / 2)).expect("encode");
    let decoded = config.decode(&token).expect("decode");
    assert_eq!(decoded.sub, "alice");

    unsafe { std::env::remove_var("DOCKER_TOKEN_SECRET") };
}
