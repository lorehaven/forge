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
    unsafe { std::env::set_var("GATEHOUSE_URL", "https://gatehouse.test") };
    unsafe { std::env::set_var("CLIENT_SECRET_SAGE_SWITCHBOARD", "test-secret") };
}

fn config_with_providers(providers: Vec<String>) -> SageConfig {
    // `SageConfig` has no small test constructor, and `load()` reads a
    // pile of env vars and a system-prompt file - assembling one field at a
    // time via `serde_json` round-trips through its `Serialize`/
    // `Deserialize` derive is cheaper than fighting `load()`'s environment.
    let base = serde_json::json!({
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
    serde_json::from_value(base).expect("valid SageConfig shape")
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // single-threaded test, held deliberately for env-var safety across the whole call
async fn validate_startup_skips_the_switchboard_check_when_asked() {
    let _guard = env_lock().lock().unwrap_or_else(|p| p.into_inner());
    with_switchboard_credentials();
    unsafe { std::env::set_var("SKIP_SWITCHBOARD_CHECK", "true") };

    let switchboard = SwitchboardClient::new();
    let config = config_with_providers(vec![]);

    let result = validate_startup(&switchboard, &config).await;
    assert!(result.is_ok());

    unsafe { std::env::remove_var("SKIP_SWITCHBOARD_CHECK") };
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // single-threaded test, held deliberately for env-var safety across the whole call
async fn validate_startup_validates_every_known_search_provider_without_network() {
    let _guard = env_lock().lock().unwrap_or_else(|p| p.into_inner());
    with_switchboard_credentials();
    unsafe { std::env::set_var("SKIP_SWITCHBOARD_CHECK", "true") };
    unsafe { std::env::remove_var("BRAVE_API_KEY") };
    unsafe { std::env::remove_var("SERPAPI_API_KEY") };

    let switchboard = SwitchboardClient::new();
    let config = config_with_providers(vec![
        "brave".to_string(),
        "searxng".to_string(),
        "serpapi".to_string(),
        "duckduckgo".to_string(),
        "some-unknown-provider".to_string(),
    ]);

    let result = validate_startup(&switchboard, &config).await;
    assert!(result.is_ok());

    unsafe { std::env::remove_var("SKIP_SWITCHBOARD_CHECK") };
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // single-threaded test, held deliberately for env-var safety across the whole call
async fn validate_startup_still_succeeds_when_provider_api_keys_are_configured() {
    let _guard = env_lock().lock().unwrap_or_else(|p| p.into_inner());
    with_switchboard_credentials();
    unsafe { std::env::set_var("SKIP_SWITCHBOARD_CHECK", "true") };
    unsafe { std::env::set_var("BRAVE_API_KEY", "key") };
    unsafe { std::env::set_var("SERPAPI_API_KEY", "key") };
    unsafe { std::env::set_var("SEARXNG_INSTANCE_URL", "https://searx.example") };

    let switchboard = SwitchboardClient::new();
    let config = config_with_providers(vec![
        "brave".to_string(),
        "searxng".to_string(),
        "serpapi".to_string(),
    ]);

    let result = validate_startup(&switchboard, &config).await;
    assert!(result.is_ok());

    unsafe { std::env::remove_var("SKIP_SWITCHBOARD_CHECK") };
    unsafe { std::env::remove_var("BRAVE_API_KEY") };
    unsafe { std::env::remove_var("SERPAPI_API_KEY") };
    unsafe { std::env::remove_var("SEARXNG_INSTANCE_URL") };
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // single-threaded test, held deliberately for env-var safety across the whole call
async fn validate_startup_fails_when_the_switchboard_check_runs_and_switchboard_is_unreachable() {
    let _guard = env_lock().lock().unwrap_or_else(|p| p.into_inner());
    with_switchboard_credentials();
    unsafe { std::env::remove_var("SKIP_SWITCHBOARD_CHECK") };
    unsafe { std::env::set_var("SWITCHBOARD_URL", "http://127.0.0.1:1") };

    let switchboard = SwitchboardClient::new();
    let config = config_with_providers(vec![]);

    let result = validate_startup(&switchboard, &config).await;
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Failed to connect to Switchboard")
    );

    unsafe { std::env::remove_var("SWITCHBOARD_URL") };
}
