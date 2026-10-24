//! `startup/validate.rs`. `validate_switchboard` makes a real HTTP call with
//! no injectable client, so it's exercised only via `SKIP_SWITCHBOARD_CHECK`,
//! which is exactly the escape hatch the source code itself documents as
//! being "for testing". `validate_search_providers` never makes a network
//! call (it only checks whether API-key env vars are set), so every branch
//! of it is reachable this way too.

use crate::env_support::env_lock;
use sage_service::clients::switchboard::SwitchboardClient;
use sage_service::config::SageConfig;
use sage_service::startup::validate_startup;

/// `SwitchboardClient::new()` panics unless these two are set - a fixed-value,
/// set-and-never-remove convention (like `docker_token`'s `TEST_KEY_MATERIAL`
/// elsewhere in this workspace) since every test in this file needs them and
/// nothing here reads their actual value.
fn with_switchboard_credentials() {
    quench_core::env::set("GATEHOUSE_URL", "https://gatehouse.test");
    quench_core::env::set("CLIENT_SECRET_SAGE_SWITCHBOARD", "test-secret");
}

fn config_with_providers(providers: &[&str]) -> SageConfig {
    // `SageConfig` has no small test constructor, and `load()` reads a
    // pile of env vars and a system-prompt file - assembling one field at a
    // time via `quench_serde` round-trips through its `Serialize`/
    // `Deserialize` derive is cheaper than fighting `load()`'s environment.
    let base = quench_serde::json!({
        "system_prompt": "test prompt",
        "default_models": [],
        "supported_models": [],
        "default_search_provider": "duckduckgo",
        "available_search_providers": providers,
        "capability_profile": {
            "name": "web_assistant",
            "description": "test",
            "enabled_tools": [],
            "default_timeout_secs": 60,
            "tool_configs": {}
        },
        "stop_models_on_shutdown": false
    });
    quench_serde::from_value(&base).expect("valid SageConfig shape")
}

#[quench_test::test]
#[allow(clippy::await_holding_lock)] // single-threaded test, held deliberately for env-var safety across the whole call
async fn validate_startup_skips_the_switchboard_check_when_asked() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    with_switchboard_credentials();
    quench_core::env::set("SKIP_SWITCHBOARD_CHECK", "true");

    let switchboard = SwitchboardClient::new();
    let config = config_with_providers(&[]);

    let result = validate_startup(&switchboard, &config).await;
    assert!(result.is_ok());

    quench_core::env::remove("SKIP_SWITCHBOARD_CHECK");
}

#[quench_test::test]
#[allow(clippy::await_holding_lock)] // single-threaded test, held deliberately for env-var safety across the whole call
async fn validate_startup_validates_every_known_search_provider_without_network() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    with_switchboard_credentials();
    quench_core::env::set("SKIP_SWITCHBOARD_CHECK", "true");
    quench_core::env::remove("BRAVE_API_KEY");
    quench_core::env::remove("SERPAPI_API_KEY");

    let switchboard = SwitchboardClient::new();
    let config = config_with_providers(&[
        "brave",
        "searxng",
        "serpapi",
        "duckduckgo",
        "some-unknown-provider",
    ]);

    let result = validate_startup(&switchboard, &config).await;
    assert!(result.is_ok());

    quench_core::env::remove("SKIP_SWITCHBOARD_CHECK");
}

#[quench_test::test]
#[allow(clippy::await_holding_lock)] // single-threaded test, held deliberately for env-var safety across the whole call
async fn validate_startup_still_succeeds_when_provider_api_keys_are_configured() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    with_switchboard_credentials();
    quench_core::env::set("SKIP_SWITCHBOARD_CHECK", "true");
    quench_core::env::set("BRAVE_API_KEY", "key");
    quench_core::env::set("SERPAPI_API_KEY", "key");
    quench_core::env::set("SEARXNG_INSTANCE_URL", "https://searx.example");

    let switchboard = SwitchboardClient::new();
    let config = config_with_providers(&["brave", "searxng", "serpapi"]);

    let result = validate_startup(&switchboard, &config).await;
    assert!(result.is_ok());

    quench_core::env::remove("SKIP_SWITCHBOARD_CHECK");
    quench_core::env::remove("BRAVE_API_KEY");
    quench_core::env::remove("SERPAPI_API_KEY");
    quench_core::env::remove("SEARXNG_INSTANCE_URL");
}

#[quench_test::test]
#[allow(clippy::await_holding_lock)] // single-threaded test, held deliberately for env-var safety across the whole call
async fn validate_startup_fails_when_the_switchboard_check_runs_and_switchboard_is_unreachable() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    with_switchboard_credentials();
    quench_core::env::remove("SKIP_SWITCHBOARD_CHECK");
    quench_core::env::set("SWITCHBOARD_URL", "http://127.0.0.1:1");

    let switchboard = SwitchboardClient::new();
    let config = config_with_providers(&[]);

    let result = validate_startup(&switchboard, &config).await;
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Failed to connect to Switchboard")
    );

    quench_core::env::remove("SWITCHBOARD_URL");
}
