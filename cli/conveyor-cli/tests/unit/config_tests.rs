use conveyor_cli::config::{FileConfig, config_path, non_empty_env};
use conveyor_cli::test_support::{ENV_LOCK, EnvGuard};
use std::path::PathBuf;

#[test]
fn non_empty_env_is_none_for_a_missing_var() {
    let _lock = ENV_LOCK.blocking_lock();
    let _guard = EnvGuard::unset("CONVEYOR_CONFIG_TEST_MISSING");
    assert_eq!(non_empty_env("CONVEYOR_CONFIG_TEST_MISSING"), None);
}

#[test]
fn non_empty_env_is_none_for_an_empty_var() {
    let _lock = ENV_LOCK.blocking_lock();
    let _guard = EnvGuard::set("CONVEYOR_CONFIG_TEST_EMPTY", "");
    assert_eq!(non_empty_env("CONVEYOR_CONFIG_TEST_EMPTY"), None);
}

#[test]
fn non_empty_env_returns_a_set_value() {
    let _lock = ENV_LOCK.blocking_lock();
    let _guard = EnvGuard::set("CONVEYOR_CONFIG_TEST_SET", "hello");
    assert_eq!(
        non_empty_env("CONVEYOR_CONFIG_TEST_SET"),
        Some("hello".to_string())
    );
}

#[test]
fn config_path_prefers_xdg_config_home_over_home() {
    let _lock = ENV_LOCK.blocking_lock();
    let _xdg = EnvGuard::set("XDG_CONFIG_HOME", "/xdg-root");
    let _home = EnvGuard::set("HOME", "/home-root");

    assert_eq!(
        config_path(),
        Some(PathBuf::from("/xdg-root/conveyor/config.toml"))
    );
}

#[test]
fn config_path_falls_back_to_home_without_xdg() {
    let _lock = ENV_LOCK.blocking_lock();
    let _xdg = EnvGuard::unset("XDG_CONFIG_HOME");
    let _home = EnvGuard::set("HOME", "/home-root");

    assert_eq!(
        config_path(),
        Some(PathBuf::from("/home-root/.config/conveyor/config.toml"))
    );
}

#[test]
fn config_path_is_none_without_either_variable() {
    let _lock = ENV_LOCK.blocking_lock();
    let _xdg = EnvGuard::unset("XDG_CONFIG_HOME");
    let _home = EnvGuard::unset("HOME");

    assert_eq!(config_path(), None);
}

#[test]
fn config_path_treats_an_empty_xdg_as_unset() {
    let _lock = ENV_LOCK.blocking_lock();
    let _xdg = EnvGuard::set("XDG_CONFIG_HOME", "");
    let _home = EnvGuard::set("HOME", "/home-root");

    assert_eq!(
        config_path(),
        Some(PathBuf::from("/home-root/.config/conveyor/config.toml"))
    );
}

#[test]
fn load_returns_defaults_when_no_config_file_exists() {
    let _lock = ENV_LOCK.blocking_lock();
    let dir = std::env::temp_dir().join(format!(
        "conveyor-config-test-missing-{}",
        std::process::id()
    ));
    let _xdg = EnvGuard::set("XDG_CONFIG_HOME", &dir.display().to_string());

    let config = FileConfig::load().unwrap();
    assert_eq!(config.url, None);
    assert!(!config.insecure);
}

#[test]
fn load_parses_a_valid_config_file() {
    let _lock = ENV_LOCK.blocking_lock();
    let dir =
        std::env::temp_dir().join(format!("conveyor-config-test-valid-{}", std::process::id()));
    let conveyor_dir = dir.join("conveyor");
    std::fs::create_dir_all(&conveyor_dir).unwrap();
    std::fs::write(
        conveyor_dir.join("config.toml"),
        "url = \"https://localhost:9443/conveyor\"\ninsecure = true\n",
    )
    .unwrap();
    let _xdg = EnvGuard::set("XDG_CONFIG_HOME", &dir.display().to_string());

    let config = FileConfig::load().unwrap();
    assert_eq!(
        config.url.as_deref(),
        Some("https://localhost:9443/conveyor")
    );
    assert!(config.insecure);

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn load_errors_on_malformed_toml() {
    let _lock = ENV_LOCK.blocking_lock();
    let dir = std::env::temp_dir().join(format!(
        "conveyor-config-test-malformed-{}",
        std::process::id()
    ));
    let conveyor_dir = dir.join("conveyor");
    std::fs::create_dir_all(&conveyor_dir).unwrap();
    std::fs::write(conveyor_dir.join("config.toml"), "not = [valid").unwrap();
    let _xdg = EnvGuard::set("XDG_CONFIG_HOME", &dir.display().to_string());

    let err = FileConfig::load().unwrap_err();
    assert!(err.to_string().contains("failed to parse"));

    std::fs::remove_dir_all(&dir).ok();
}
