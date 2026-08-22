use std::sync::{Mutex, OnceLock};
use welder::config::app_config::{BackendConfig, Config, is_verbose};

/// `WELDER_DEBUG` is process-global and read by `is_verbose`, which every
/// test in this binary can call indirectly - any test that sets or removes
/// it must hold this for the duration.
fn welder_debug_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// Holds the crate-wide `support::cwd_lock` - shared with `backend_tests`,
/// which forces `welder::config::CONFIG` (see `support::cwd_lock`'s doc
/// comment for why a *reader* of `CONFIG` needs this too).
fn in_dir<T>(dir: &std::path::Path, body: impl FnOnce() -> T) -> T {
    let _guard = crate::support::cwd_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let original = std::env::current_dir().expect("current dir");
    std::env::set_current_dir(dir).expect("set cwd");
    let result = body();
    std::env::set_current_dir(original).expect("restore cwd");
    result
}

#[test]
fn backend_config_default_is_ollama_on_the_standard_port() {
    let config = BackendConfig::default();
    assert_eq!(config.kind, "ollama");
    assert_eq!(config.ollama_url.as_deref(), Some("127.0.0.1:11434"));
    assert!(!config.debug);
    assert!(config.switchboard_url.is_none());
    assert!(config.switchboard_tls_verify);
}

#[test]
fn config_default_wraps_backend_default() {
    let config = Config::default();
    assert_eq!(config.backend.kind, "ollama");
}

#[test]
fn is_verbose_true_when_welder_debug_env_is_set() {
    let _guard = welder_debug_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    for value in ["true", "TRUE", "1"] {
        envmnt::set("WELDER_DEBUG", value);
        assert!(is_verbose(), "WELDER_DEBUG={value}");
    }
    envmnt::remove("WELDER_DEBUG");
}

#[test]
fn load_defaults_when_no_config_file_is_present() {
    let _guard = welder_debug_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    envmnt::remove("WELDER_DEBUG");
    let dir = tempfile::tempdir().expect("tempdir");
    let config = in_dir(dir.path(), Config::load);
    assert_eq!(config.backend.kind, "ollama");
}

#[test]
fn load_parses_an_existing_config_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let welder_dir = dir.path().join(".welder");
    std::fs::create_dir_all(&welder_dir).expect("mkdir");
    std::fs::write(
        welder_dir.join("config.toml"),
        "[backend]\nkind = \"switchboard\"\ndebug = false\nswitchboard_url = \"https://sb.test\"\n",
    )
    .expect("write config");

    let config = in_dir(dir.path(), Config::load);
    assert_eq!(config.backend.kind, "switchboard");
    assert_eq!(
        config.backend.switchboard_url.as_deref(),
        Some("https://sb.test")
    );
}

#[test]
fn load_falls_back_to_defaults_on_invalid_toml() {
    let dir = tempfile::tempdir().expect("tempdir");
    let welder_dir = dir.path().join(".welder");
    std::fs::create_dir_all(&welder_dir).expect("mkdir");
    std::fs::write(welder_dir.join("config.toml"), "not valid toml {{{").expect("write config");

    let config = in_dir(dir.path(), Config::load);
    assert_eq!(config.backend.kind, "ollama");
}
