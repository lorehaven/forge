use crate::env_support::cwd_lock;
use riveter::config::{RiveterConfig, load_config, save_config};

#[quench_test::test]
fn test_default_config() {
    let config = RiveterConfig::default();
    assert!(config.env.current.is_none());
}

#[quench_test::test]
fn test_parse_config() {
    let toml_str = r#"
        [env]
        current = "prod"
    "#;
    let config: RiveterConfig = quench_serde::toml::from_str(toml_str).unwrap();
    assert_eq!(config.env.current, Some("prod".to_string()));
}

/// Runs `body` with the process cwd set to a fresh temp directory, holding
/// `cwd_lock` for the duration - `load_config`/`save_config` both read and
/// write `.riveter.toml` relative to cwd, which is process-global state.
fn in_temp_cwd<T>(body: impl FnOnce() -> T) -> T {
    let _guard = cwd_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let dir = quench_core::tmp::TempDir::new().unwrap();
    let original = std::env::current_dir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();
    let result = body();
    std::env::set_current_dir(original).unwrap();
    result
}

#[quench_test::test]
fn load_config_defaults_when_no_file_exists() {
    in_temp_cwd(|| {
        let config = load_config().unwrap();
        assert!(config.env.current.is_none());
    });
}

#[quench_test::test]
fn save_then_load_round_trips_the_current_environment() {
    in_temp_cwd(|| {
        let mut config = RiveterConfig::default();
        config.env.current = Some("staging".to_string());
        save_config(&config).unwrap();

        let loaded = load_config().unwrap();
        assert_eq!(loaded.env.current, Some("staging".to_string()));
    });
}

#[quench_test::test]
fn load_config_reports_invalid_toml() {
    in_temp_cwd(|| {
        std::fs::write(".riveter.toml", "not = [valid").unwrap();
        let error = load_config().unwrap_err();
        assert!(error.to_string().contains("Failed to parse"));
    });
}

#[quench_test::test]
fn the_warehouse_url_is_read_and_survives_a_save() {
    in_temp_cwd(|| {
        std::fs::write(
            ".riveter.toml",
            "warehouse_url = \"https://ennor.ddns.net/warehouse\"\n\n[env]\ncurrent = \"forge\"\n",
        )
        .unwrap();
        let mut config = load_config().unwrap();
        assert_eq!(
            config.warehouse_url.as_deref(),
            Some("https://ennor.ddns.net/warehouse")
        );

        // `riveter env use` loads and saves the file; the URL must not be dropped by that.
        config.env.current = Some("media".to_string());
        save_config(&config).unwrap();
        let reloaded = load_config().unwrap();
        assert_eq!(
            reloaded.warehouse_url.as_deref(),
            Some("https://ennor.ddns.net/warehouse")
        );
        assert_eq!(reloaded.env.current.as_deref(), Some("media"));
    });
}

#[quench_test::test]
fn a_config_without_a_warehouse_url_still_parses() {
    let config: RiveterConfig = quench_serde::toml::from_str("[env]\ncurrent = \"x\"\n").unwrap();
    assert!(config.warehouse_url.is_none());
}
