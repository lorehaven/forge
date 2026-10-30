use anvil::commands::login::*;
use gatehouse_auth::Credentials;

#[quench_test::test]
fn registry_token_var_follows_cargos_naming() {
    assert_eq!(registry_token_var("ennor"), "CARGO_REGISTRIES_ENNOR_TOKEN");
    assert_eq!(
        registry_token_var("my-registry"),
        "CARGO_REGISTRIES_MY_REGISTRY_TOKEN"
    );
}

#[quench_test::test]
fn default_token_name_mentions_the_host() {
    assert_eq!(default_token_name("forge-1"), "anvil on forge-1");
}

#[quench_test::test]
fn registry_token_env_is_none_without_a_secret_file() {
    let dir = quench_core::tmp::TempDir::new().unwrap();
    let result = registry_token_env("ennor", dir.path(), |_| false).unwrap();
    assert!(result.is_none());
}

#[quench_test::test]
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

#[quench_test::test]
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

#[quench_test::test]
fn credentials_come_from_the_environment_when_there_is_no_secret_file() {
    let dir = quench_core::tmp::TempDir::new().unwrap();
    let env = |name: &str| match name {
        "ANVIL_APP_TOKEN" => Some(" lht_ci \n".to_string()),
        "ANVIL_GATEHOUSE_URL" => Some("https://host/gatehouse".to_string()),
        _ => None,
    };
    let credentials = credentials_from(dir.path(), env).unwrap().unwrap();
    assert_eq!(credentials.app_token, "lht_ci");
    assert_eq!(credentials.gatehouse_url, "https://host/gatehouse");
}

#[quench_test::test]
fn half_an_environment_falls_back_to_the_secret_file() {
    let dir = quench_core::tmp::TempDir::new().unwrap();
    let env = |name: &str| (name == "ANVIL_APP_TOKEN").then(|| "lht_ci".to_string());
    assert!(credentials_from(dir.path(), env).unwrap().is_none());
}

#[quench_test::test]
fn registry_token_without_credentials_says_how_to_get_some() {
    let dir = quench_core::tmp::TempDir::new().unwrap();
    let error = registry_token(dir.path(), |_| None)
        .unwrap_err()
        .to_string();
    assert!(error.contains("anvil login"), "{error}");
    assert!(error.contains("ANVIL_APP_TOKEN"), "{error}");
}

#[quench_test::test]
fn registry_token_reports_an_unreachable_gatehouse() {
    let dir = quench_core::tmp::TempDir::new().unwrap();
    let env = |name: &str| match name {
        "ANVIL_APP_TOKEN" => Some("lht_ci".to_string()),
        "ANVIL_GATEHOUSE_URL" => Some("http://127.0.0.1:1".to_string()),
        _ => None,
    };
    assert!(registry_token(dir.path(), env).is_err());
}

#[quench_test::test]
fn the_build_gets_the_stored_signing_passwords_unless_the_environment_has_them() {
    let dir = quench_core::tmp::TempDir::new().unwrap();
    assert!(keystore_env(dir.path(), |_| false).unwrap().is_empty());

    let credentials = Credentials {
        gatehouse_url: "https://host/gatehouse".to_string(),
        app_token: "lht_secret".to_string(),
    };
    gatehouse_auth::save("anvil", dir.path(), &credentials).unwrap();
    gatehouse_auth::set_extra("anvil", dir.path(), "android", "keystore_password", "ks").unwrap();
    gatehouse_auth::set_extra("anvil", dir.path(), "android", "key_password", "key").unwrap();

    let all = keystore_env(dir.path(), |_| false).unwrap();
    assert_eq!(
        all,
        [
            ("QUENCH_KEYSTORE_PASSWORD", "ks".to_string()),
            ("QUENCH_KEY_PASSWORD", "key".to_string())
        ]
    );
    let rest = keystore_env(dir.path(), |v| v == "QUENCH_KEYSTORE_PASSWORD").unwrap();
    assert_eq!(rest, [("QUENCH_KEY_PASSWORD", "key".to_string())]);
}
