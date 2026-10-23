use anvil::commands::login::*;
use gatehouse_auth::Credentials;

#[test]
fn registry_token_var_follows_cargos_naming() {
    assert_eq!(registry_token_var("ennor"), "CARGO_REGISTRIES_ENNOR_TOKEN");
    assert_eq!(
        registry_token_var("my-registry"),
        "CARGO_REGISTRIES_MY_REGISTRY_TOKEN"
    );
}

#[test]
fn default_token_name_mentions_the_host() {
    assert_eq!(default_token_name("forge-1"), "anvil on forge-1");
}

#[test]
fn registry_token_env_is_none_without_a_secret_file() {
    let dir = quench_core::tmp::TempDir::new().unwrap();
    let result = registry_token_env("ennor", dir.path(), |_| false).unwrap();
    assert!(result.is_none());
}

#[test]
fn registry_token_env_defers_to_a_token_already_in_the_environment() {
    let dir = quench_core::tmp::TempDir::new().unwrap();
    gatehouse_auth::save(
        "anvil",
        dir.path(),
        &Credentials {
            gatehouse_url: "http://127.0.0.1:1".to_string(),
            app_token: "lht_x".to_string(),
        },
    )
    .unwrap();
    // The unreachable Gatehouse is never asked, because the env var wins.
    let result = registry_token_env("ennor", dir.path(), |name| {
        name == "CARGO_REGISTRIES_ENNOR_TOKEN"
    })
    .unwrap();
    assert!(result.is_none());
}

#[test]
fn registry_token_env_reports_an_unreachable_gatehouse() {
    let dir = quench_core::tmp::TempDir::new().unwrap();
    gatehouse_auth::save(
        "anvil",
        dir.path(),
        &Credentials {
            gatehouse_url: "http://127.0.0.1:1".to_string(),
            app_token: "lht_x".to_string(),
        },
    )
    .unwrap();
    let err = registry_token_env("ennor", dir.path(), |_| false).unwrap_err();
    assert!(err.to_string().contains("access token"));
}
