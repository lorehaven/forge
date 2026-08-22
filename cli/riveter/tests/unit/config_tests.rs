use crate::env_support::cwd_lock;
use riveter::config::{RiveterConfig, load_config, save_config};

#[test]
fn test_default_config() {
    let config = RiveterConfig::default();
    assert!(config.env.current.is_none());
}

#[test]
fn test_parse_config() {
    let toml_str = r#"
        [env]
        current = "prod"
    "#;
    let config: RiveterConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(config.env.current, Some("prod".to_string()));
}

/// Runs `body` with the process cwd set to a fresh temp directory, holding
/// `cwd_lock` for the duration - `load_config`/`save_config` both read and
/// write `.riveter.toml` relative to cwd, which is process-global state.
fn in_temp_cwd<T>(body: impl FnOnce() -> T) -> T {
    let _guard = cwd_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let dir = tempfile::tempdir().unwrap();
    let original = std::env::current_dir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();
    let result = body();
    std::env::set_current_dir(original).unwrap();
    result
}

#[test]
fn load_config_defaults_when_no_file_exists() {
    in_temp_cwd(|| {
        let config = load_config().unwrap();
        assert!(config.env.current.is_none());
    });
}

#[test]
fn save_then_load_round_trips_the_current_environment() {
    in_temp_cwd(|| {
        let mut config = RiveterConfig::default();
        config.env.current = Some("staging".to_string());
        save_config(&config).unwrap();

        let loaded = load_config().unwrap();
        assert_eq!(loaded.env.current, Some("staging".to_string()));
    });
}

#[test]
fn load_config_reports_invalid_toml() {
    in_temp_cwd(|| {
        std::fs::write(".riveter.toml", "not = [valid").unwrap();
        let error = load_config().unwrap_err();
        assert!(error.to_string().contains("Failed to parse"));
    });
}
