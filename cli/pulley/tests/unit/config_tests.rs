use crate::env_support::{ENV_LOCK, EnvGuard};
use pulley::config::{Config, ConfigError, Job, home_dir};
use std::path::PathBuf;

fn scratch(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("pulley-config-test-{name}-{}", std::process::id()))
}

fn job(id: &str) -> Job {
    Job {
        id: id.to_string(),
        desc: "desc".to_string(),
        src: "/src".to_string(),
        dest: "/dest".to_string(),
        delete: false,
        skip: Vec::new(),
        no_confirm: false,
        interval: None,
    }
}

#[test]
fn from_file_errors_when_the_file_is_missing() {
    let path = scratch("missing.toml");
    let err = Config::from_file(&path).unwrap_err();
    assert!(matches!(err, ConfigError::NotFound(_)));
}

#[test]
fn from_file_errors_on_malformed_toml() {
    let path = scratch("malformed.toml");
    std::fs::write(&path, "not = [valid").unwrap();
    let err = Config::from_file(&path).unwrap_err();
    assert!(matches!(err, ConfigError::ParseError(_)));
    std::fs::remove_file(&path).ok();
}

#[test]
fn from_file_parses_a_valid_job() {
    let path = scratch("valid.toml");
    std::fs::write(
        &path,
        r#"
        [[jobs]]
        id = "photos"
        desc = "backup photos"
        src = "/home/me/photos"
        dest = "/backup/photos"
        interval = 3600
        "#,
    )
    .unwrap();

    let config = Config::from_file(&path).unwrap();
    assert_eq!(config.jobs.len(), 1);
    assert_eq!(config.jobs[0].id, "photos");
    assert_eq!(config.jobs[0].interval, Some(3600));
    assert!(!config.jobs[0].delete);
    assert!(!config.jobs[0].no_confirm);

    std::fs::remove_file(&path).ok();
}

#[test]
fn merge_job_appends_a_new_id() {
    let mut jobs = vec![job("a"), job("b")];
    Config::merge_job(&mut jobs, job("c"));
    assert_eq!(jobs.len(), 3);
    assert_eq!(jobs[2].id, "c");
}

#[test]
fn merge_job_overwrites_an_existing_id_in_place() {
    let mut jobs = vec![job("a"), job("b")];
    let mut replacement = job("b");
    replacement.desc = "replaced".to_string();

    Config::merge_job(&mut jobs, replacement);

    assert_eq!(jobs.len(), 2);
    assert_eq!(jobs[1].desc, "replaced");
}

#[test]
fn home_dir_prefers_home_over_userprofile() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _home = EnvGuard::set("HOME", "/home/me");
    let _profile = EnvGuard::set("USERPROFILE", "C:\\Users\\me");

    assert_eq!(home_dir(), Some(PathBuf::from("/home/me")));
}

#[test]
fn home_dir_falls_back_to_userprofile_without_home() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _home = EnvGuard::unset("HOME");
    let _profile = EnvGuard::set("USERPROFILE", "C:\\Users\\me");

    assert_eq!(home_dir(), Some(PathBuf::from("C:\\Users\\me")));
}

#[test]
fn home_dir_is_none_without_either_variable() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _home = EnvGuard::unset("HOME");
    let _profile = EnvGuard::unset("USERPROFILE");

    assert_eq!(home_dir(), None);
}

#[test]
fn find_local_configs_matches_only_dot_pulley_toml_suffix() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let dir = scratch("local-configs-dir");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("backup.pulley.toml"), "jobs = []").unwrap();
    std::fs::write(dir.join("unrelated.toml"), "jobs = []").unwrap();
    std::fs::write(dir.join("also.pulley.toml"), "jobs = []").unwrap();

    let original_cwd = std::env::current_dir().unwrap();
    std::env::set_current_dir(&dir).unwrap();
    let found = Config::find_local_configs();
    std::env::set_current_dir(original_cwd).unwrap();

    let names: Vec<String> = found
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
        .collect();
    assert_eq!(names, vec!["also.pulley.toml", "backup.pulley.toml"]);

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn find_global_configs_is_empty_when_the_global_config_dir_does_not_exist() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let dir = scratch("no-such-home");
    std::fs::remove_dir_all(&dir).ok();
    let _home = EnvGuard::set("HOME", dir.to_str().unwrap());

    assert!(Config::find_global_configs().is_empty());
}

#[test]
fn load_merged_errors_when_no_configs_are_found_anywhere() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let home = scratch("empty-home");
    std::fs::remove_dir_all(&home).ok();
    let _home_guard = EnvGuard::set("HOME", home.to_str().unwrap());

    let empty_cwd = scratch("empty-cwd");
    std::fs::create_dir_all(&empty_cwd).unwrap();
    let original_cwd = std::env::current_dir().unwrap();
    std::env::set_current_dir(&empty_cwd).unwrap();

    let result = Config::load_merged();

    std::env::set_current_dir(original_cwd).unwrap();
    std::fs::remove_dir_all(&empty_cwd).ok();

    assert!(matches!(result, Err(ConfigError::NotFound(_))));
}

#[test]
fn load_merged_reads_local_configs_and_reports_their_jobs() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let home = scratch("home-without-globals");
    std::fs::remove_dir_all(&home).ok();
    let _home_guard = EnvGuard::set("HOME", home.to_str().unwrap());

    let cwd = scratch("cwd-with-local-config");
    std::fs::create_dir_all(&cwd).unwrap();
    std::fs::write(
        cwd.join("backup.pulley.toml"),
        r#"
        [[jobs]]
        id = "docs"
        desc = "backup docs"
        src = "/docs"
        dest = "/backup/docs"
        "#,
    )
    .unwrap();

    let original_cwd = std::env::current_dir().unwrap();
    std::env::set_current_dir(&cwd).unwrap();

    let config = Config::load_merged();

    std::env::set_current_dir(original_cwd).unwrap();
    std::fs::remove_dir_all(&cwd).ok();

    let config = config.unwrap();
    assert_eq!(config.jobs.len(), 1);
    assert_eq!(config.jobs[0].id, "docs");
}
