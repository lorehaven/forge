use anvil::commands::docker::{
    cargo_home, cargo_registry_index, cargo_registry_token, env_var_name_for_registry,
    find_module_for_package, find_module_name_overwrite, full_tags_for_package,
    get_build_args_for_package, get_dockerfile_for_package, get_image_name_for_package,
    get_registries_for_package, process_all_packages,
};
use anvil::config::Config;
use std::path::PathBuf;

use crate::support;
use support::stable_cwd_lock;

// `CARGO_HOME`/`CARGO_REGISTRIES_*` are process-global just like cwd, and
// real `cargo metadata` shell-outs elsewhere in this binary (see
// `stable_cwd_lock`'s docs) depend on them - a test here setting
// `CARGO_HOME` to a nonexistent path, or unsetting
// `CARGO_REGISTRIES_ENNOR_INDEX`/`_TOKEN`, would break a concurrently
// spawned `cargo metadata` child process if it weren't serialized against
// the very same lock those tests hold.

fn config_from_toml(toml_str: &str) -> Config {
    toml::from_str(toml_str).expect("valid config toml")
}

fn sample_config() -> Config {
    config_from_toml(
        r#"
        [docker]
        registry = "ghcr.io/acme"

        [docker.modules.core]
        packages = ["service-a", "service-b"]
        dockerfile = "Dockerfile.core"

        [docker.modules.core.service-a]
        dockerfile = "Dockerfile.service-a"
        module_name = "custom-module"
        image_name = "svc-a"
        registries = ["registry.internal", "backup.internal"]
        build_args = { RESOURCE_DIR = "migrations" }
        "#,
    )
}

#[test]
fn find_module_for_package_locates_the_owning_module() {
    let config = sample_config();
    assert_eq!(
        find_module_for_package(&config, "service-a").unwrap(),
        "core"
    );
    assert_eq!(
        find_module_for_package(&config, "service-b").unwrap(),
        "core"
    );
}

#[test]
fn find_module_for_package_errors_for_an_unknown_package() {
    let config = sample_config();
    assert!(find_module_for_package(&config, "not-configured").is_err());
}

#[test]
fn find_module_name_overwrite_uses_the_override_when_present() {
    let config = sample_config();
    assert_eq!(
        find_module_name_overwrite(&config, "service-a").unwrap(),
        "custom-module"
    );
}

#[test]
fn find_module_name_overwrite_falls_back_to_the_module_name() {
    let config = sample_config();
    assert_eq!(
        find_module_name_overwrite(&config, "service-b").unwrap(),
        "core"
    );
}

#[test]
fn get_dockerfile_for_package_prefers_the_package_override() {
    let config = sample_config();
    assert_eq!(
        get_dockerfile_for_package(&config, "service-a").unwrap(),
        "Dockerfile.service-a"
    );
}

#[test]
fn get_dockerfile_for_package_falls_back_to_the_module_dockerfile() {
    let config = sample_config();
    assert_eq!(
        get_dockerfile_for_package(&config, "service-b").unwrap(),
        "Dockerfile.core"
    );
}

#[test]
fn get_build_args_for_package_returns_the_overrides_map() {
    let config = sample_config();
    let args = get_build_args_for_package(&config, "service-a").unwrap();
    assert_eq!(
        args.get("RESOURCE_DIR").map(String::as_str),
        Some("migrations")
    );
}

#[test]
fn get_build_args_for_package_is_empty_without_an_override() {
    let config = sample_config();
    let args = get_build_args_for_package(&config, "service-b").unwrap();
    assert!(args.is_empty());
}

#[test]
fn get_image_name_for_package_prefers_the_override() {
    let config = sample_config();
    assert_eq!(
        get_image_name_for_package(&config, "service-a").unwrap(),
        "svc-a"
    );
}

#[test]
fn get_image_name_for_package_defaults_to_the_package_name() {
    let config = sample_config();
    assert_eq!(
        get_image_name_for_package(&config, "service-b").unwrap(),
        "service-b"
    );
}

#[test]
fn get_registries_for_package_prefers_the_override_list() {
    let config = sample_config();
    assert_eq!(
        get_registries_for_package(&config, "service-a").unwrap(),
        vec![
            "registry.internal".to_string(),
            "backup.internal".to_string()
        ]
    );
}

#[test]
fn get_registries_for_package_falls_back_to_the_global_registry() {
    let config = sample_config();
    assert_eq!(
        get_registries_for_package(&config, "service-b").unwrap(),
        vec!["ghcr.io/acme".to_string()]
    );
}

#[test]
fn get_registries_for_package_falls_back_to_the_deprecated_single_registry_override() {
    let config = config_from_toml(
        r#"
        [docker.modules.core]
        packages = ["service-c"]
        dockerfile = "Dockerfile.core"

        [docker.modules.core.service-c]
        registry = "legacy.internal"
        "#,
    );
    assert_eq!(
        get_registries_for_package(&config, "service-c").unwrap(),
        vec!["legacy.internal".to_string()]
    );
}

#[test]
fn get_registries_for_package_errors_when_nothing_is_configured_anywhere() {
    let config = config_from_toml(
        r#"
        [docker.modules.core]
        packages = ["service-d"]
        dockerfile = "Dockerfile.core"
        "#,
    );
    let error = get_registries_for_package(&config, "service-d").unwrap_err();
    assert!(error.to_string().contains("No Docker registry configured"));
}

#[test]
fn get_registries_for_package_errors_when_the_configured_registries_are_blank() {
    let config = config_from_toml(
        r#"
        [docker.modules.core]
        packages = ["service-e"]
        dockerfile = "Dockerfile.core"

        [docker.modules.core.service-e]
        registries = ["   "]
        "#,
    );
    let error = get_registries_for_package(&config, "service-e").unwrap_err();
    assert!(error.to_string().contains("are empty"));
}

#[test]
fn env_var_name_for_registry_uppercases_and_replaces_hyphens() {
    assert_eq!(
        env_var_name_for_registry("my-private-registry", "INDEX"),
        "CARGO_REGISTRIES_MY_PRIVATE_REGISTRY_INDEX"
    );
    assert_eq!(
        env_var_name_for_registry("ennor", "TOKEN"),
        "CARGO_REGISTRIES_ENNOR_TOKEN"
    );
}

#[test]
fn cargo_home_prefers_cargo_home_env_var() {
    let _guard = stable_cwd_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    envmnt::set("CARGO_HOME", "/explicit/cargo/home");
    assert_eq!(cargo_home(), Some(PathBuf::from("/explicit/cargo/home")));
    envmnt::remove("CARGO_HOME");
}

#[test]
fn cargo_home_falls_back_to_home_joined_with_dot_cargo() {
    let _guard = stable_cwd_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let real_home = std::env::var("HOME").ok();
    envmnt::remove("CARGO_HOME");
    envmnt::set("HOME", "/home/example");

    assert_eq!(cargo_home(), Some(PathBuf::from("/home/example/.cargo")));

    match real_home {
        Some(home) => envmnt::set("HOME", home),
        None => envmnt::remove("HOME"),
    }
}

#[test]
fn cargo_registry_index_prefers_the_env_var_override() {
    let _guard = stable_cwd_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    envmnt::set(
        "CARGO_REGISTRIES_ENNOR_INDEX",
        "sparse+https://env-wins/index/",
    );
    let config = sample_config();
    assert_eq!(
        cargo_registry_index(&config, "ennor"),
        Some("sparse+https://env-wins/index/".to_string())
    );
    envmnt::remove("CARGO_REGISTRIES_ENNOR_INDEX");
}

#[test]
fn cargo_registry_index_falls_back_to_anvil_toml_when_env_is_unset() {
    let _guard = stable_cwd_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    envmnt::remove("CARGO_REGISTRIES_ENNOR_INDEX");
    let config = config_from_toml(
        r#"
        [docker]
        cargo_registry_index = "sparse+https://from-anvil-toml/index/"
        "#,
    );
    assert_eq!(
        cargo_registry_index(&config, "ennor"),
        Some("sparse+https://from-anvil-toml/index/".to_string())
    );
}

#[test]
fn cargo_registry_index_is_none_when_nothing_resolves_it() {
    let _guard = stable_cwd_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    envmnt::remove("CARGO_REGISTRIES_UNKNOWNREG_INDEX");
    envmnt::set("CARGO_HOME", "/does/not/exist/cargo/home");
    let config = sample_config();
    assert_eq!(cargo_registry_index(&config, "unknownreg"), None);
    envmnt::remove("CARGO_HOME");
}

#[test]
fn cargo_registry_token_prefers_the_env_var_and_ignores_blank_values() {
    let _guard = stable_cwd_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    envmnt::set("CARGO_REGISTRIES_ENNOR_TOKEN", "   ");
    envmnt::set("CARGO_HOME", "/does/not/exist/cargo/home");
    // A blank env var is treated as unset, so this falls through to the
    // (missing) private registry config file and resolves to None.
    assert_eq!(cargo_registry_token("ennor"), None);

    envmnt::set("CARGO_REGISTRIES_ENNOR_TOKEN", "real-token");
    assert_eq!(
        cargo_registry_token("ennor"),
        Some("real-token".to_string())
    );

    envmnt::remove("CARGO_REGISTRIES_ENNOR_TOKEN");
    envmnt::remove("CARGO_HOME");
}

#[test]
fn full_tags_for_package_combines_registry_module_image_and_version() {
    // `full_tags_for_package` calls `cargo_meta::resolve_package`, which
    // shells out to real `cargo metadata` with no explicit
    // `--manifest-path` - needs cwd to stay put, see `stable_cwd_lock`'s
    // docs - and the package name here has to be a real workspace
    // member. `anvil` (this very crate) works and is configured under a
    // fake module name/registry for the test.
    let _cwd_guard = stable_cwd_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let config = config_from_toml(
        r#"
        [docker.modules.tools]
        packages = ["anvil"]
        dockerfile = "Dockerfile.tools"

        [docker.modules.tools.anvil]
        image_name = "anvil-image"
        registries = ["registry.internal"]
        "#,
    );
    let tags = full_tags_for_package(&config, "anvil").expect("anvil resolves in this workspace");
    assert_eq!(tags.len(), 1);
    assert!(tags[0].starts_with("registry.internal/tools/anvil-image:"));
}

#[test]
fn process_all_packages_reports_success_when_every_op_succeeds() {
    let config = sample_config();
    let mut seen = Vec::new();
    let result = process_all_packages(
        &config,
        |package| {
            seen.push(package.to_string());
            Ok(())
        },
        "test-op",
    );
    assert!(result.is_ok());
    seen.sort();
    assert_eq!(seen, vec!["service-a".to_string(), "service-b".to_string()]);
}

#[test]
fn process_all_packages_collects_every_failure_and_still_runs_the_rest() {
    let config = sample_config();
    let mut attempted = Vec::new();
    let result = process_all_packages(
        &config,
        |package| {
            attempted.push(package.to_string());
            if package == "service-a" {
                anyhow::bail!("boom");
            }
            Ok(())
        },
        "test-op",
    );
    assert!(result.is_err());
    attempted.sort();
    // Both packages were still attempted - one failure doesn't stop the rest.
    assert_eq!(
        attempted,
        vec!["service-a".to_string(), "service-b".to_string()]
    );
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("failed for 1 packages")
    );
}

#[test]
fn process_all_packages_is_a_no_op_success_for_an_empty_config() {
    let config = Config::default();
    let result = process_all_packages(&config, |_| Ok(()), "test-op");
    assert!(result.is_ok());
}
