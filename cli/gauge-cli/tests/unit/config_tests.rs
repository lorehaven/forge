use gauge_cli::config::{FileConfig, config_path};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gauge-cli-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[quench_test::test]
fn the_path_follows_xdg_then_home() {
    let xdg = |k: &str| (k == "XDG_CONFIG_HOME").then(|| "/cfg".to_string());
    assert_eq!(
        config_path(xdg),
        Some(PathBuf::from("/cfg/gauge/config.toml"))
    );
    let home = |k: &str| (k == "HOME").then(|| "/home/me".to_string());
    assert_eq!(
        config_path(home),
        Some(PathBuf::from("/home/me/.config/gauge/config.toml"))
    );
    assert_eq!(config_path(|_| None), None);
}

#[quench_test::test]
fn a_missing_file_is_empty_and_a_broken_one_is_an_error() {
    let dir = scratch("load");
    let path = dir.join("gauge/config.toml");
    assert_eq!(FileConfig::load(&path).unwrap(), FileConfig::default());
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "url = [").unwrap();
    assert!(
        FileConfig::load(&path)
            .unwrap_err()
            .to_string()
            .contains("failed to parse")
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[quench_test::test]
fn a_saved_file_round_trips_and_only_the_owner_can_read_it() {
    let dir = scratch("save");
    let path = dir.join("gauge/config.toml");
    let file = FileConfig {
        url: Some("https://x.example".into()),
        path: None,
        token: Some("secret".into()),
    };
    file.save(&path).unwrap();
    assert_eq!(FileConfig::load(&path).unwrap(), file);
    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(!text.contains("path"), "unset values are left out: {text}");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[quench_test::test]
fn a_renewed_token_replaces_the_old_one_and_keeps_the_rest() {
    let dir = scratch("store");
    let path = dir.join("gauge/config.toml");
    FileConfig {
        url: Some("https://x.example".into()),
        path: Some("/g".into()),
        token: Some("old".into()),
    }
    .save(&path)
    .unwrap();
    FileConfig::store_token(&path, "new").unwrap();
    let file = FileConfig::load(&path).unwrap();
    assert_eq!(file.token.as_deref(), Some("new"));
    assert_eq!(file.url.as_deref(), Some("https://x.example"));
    assert_eq!(file.path.as_deref(), Some("/g"));
    std::fs::remove_dir_all(&dir).unwrap();
}
