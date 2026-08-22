use crate::env_support::cwd_lock;
use riveter::env::{ENV_VAR, current_env, env_list, env_set, manifest_path, resolve_env};

#[test]
fn test_manifest_path() {
    assert_eq!(manifest_path("prod"), "manifests/prod-manifests.yaml");
    assert_eq!(manifest_path("dev"), "manifests/dev-manifests.yaml");
}

/// Runs `body` in a fresh temp cwd containing `overlays/<env>/overlay.yaml`
/// for every name in `envs`, with `$RIVETER_ENV` cleared first - both cwd
/// and the env var are process-global state every test here shares.
fn in_temp_cwd_with_overlays<T>(envs: &[&str], body: impl FnOnce() -> T) -> T {
    let _guard = cwd_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let dir = tempfile::tempdir().unwrap();
    for env in envs {
        let overlay_dir = dir.path().join("overlays").join(env);
        std::fs::create_dir_all(&overlay_dir).unwrap();
        std::fs::write(overlay_dir.join("overlay.yaml"), "# overlay\n").unwrap();
    }
    let original = std::env::current_dir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();
    envmnt::remove(ENV_VAR);

    let result = body();

    envmnt::remove(ENV_VAR);
    std::env::set_current_dir(original).unwrap();
    result
}

#[test]
fn resolve_env_uses_the_explicit_name_when_the_overlay_exists() {
    in_temp_cwd_with_overlays(&["prod"], || {
        assert_eq!(resolve_env(Some("prod")).unwrap(), "prod");
    });
}

#[test]
fn resolve_env_rejects_an_explicit_name_with_no_matching_overlay() {
    in_temp_cwd_with_overlays(&[], || {
        assert!(resolve_env(Some("missing")).is_err());
    });
}

#[test]
fn resolve_env_rejects_a_blank_explicit_name() {
    in_temp_cwd_with_overlays(&[], || {
        let error = resolve_env(Some("   ")).unwrap_err();
        assert!(error.to_string().contains("needs an environment name"));
    });
}

#[test]
fn resolve_env_falls_back_to_current_env_when_nothing_explicit_is_given() {
    in_temp_cwd_with_overlays(&["dev"], || {
        env_set("dev").unwrap();
        assert_eq!(resolve_env(None).unwrap(), "dev");
    });
}

#[test]
fn current_env_prefers_the_env_var_over_the_saved_config() {
    in_temp_cwd_with_overlays(&["dev", "prod"], || {
        env_set("dev").unwrap();
        envmnt::set(ENV_VAR, "prod");
        assert_eq!(current_env().unwrap(), "prod");
    });
}

#[test]
fn current_env_errors_with_no_env_var_and_no_saved_config() {
    in_temp_cwd_with_overlays(&[], || {
        let error = current_env().unwrap_err();
        assert!(error.to_string().contains("No environment set"));
    });
}

#[test]
fn env_set_persists_the_choice_for_a_later_current_env_call() {
    in_temp_cwd_with_overlays(&["staging"], || {
        env_set("staging").unwrap();
        assert_eq!(current_env().unwrap(), "staging");
    });
}

#[test]
fn env_set_rejects_an_overlay_that_does_not_exist() {
    in_temp_cwd_with_overlays(&[], || {
        assert!(env_set("nope").is_err());
    });
}

#[test]
fn env_list_prints_every_overlay_with_an_overlay_yaml() {
    in_temp_cwd_with_overlays(&["prod", "dev"], || {
        // `env_list` only prints to stdout; this asserts it does not error
        // and successfully walks a directory containing both a valid
        // overlay (with `overlay.yaml`) and a directory entry without one.
        std::fs::create_dir_all("overlays/incomplete").unwrap();
        env_list().unwrap();
    });
}
