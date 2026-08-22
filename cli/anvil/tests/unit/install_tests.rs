use anvil::commands::install::{
    default_module_name, resolve_install_targets, resolve_package_path,
    resolve_workspace_install_targets,
};
use anvil::config::{Config, InstallConfig};
use serde_json::{Value, json};
use std::path::PathBuf;

fn config_with_packages(packages: &[&str]) -> Config {
    Config {
        install: InstallConfig {
            packages: packages.iter().map(ToString::to_string).collect(),
        },
        ..Config::default()
    }
}

fn fixture_metadata() -> Value {
    json!({
        "workspace_root": "/workspace",
        "workspace_members": ["pkg-a-id", "pkg-b-id"],
        "packages": [
            {
                "id": "pkg-a-id",
                "name": "pkg-a",
                "manifest_path": "/workspace/crates/pkg-a/Cargo.toml"
            },
            {
                "id": "pkg-b-id",
                "name": "pkg-b",
                "manifest_path": "/workspace/crates/pkg-b/Cargo.toml"
            },
            {
                "id": "external-id",
                "name": "external-dep",
                "manifest_path": "/registry/external-dep/Cargo.toml"
            }
        ]
    })
}

#[test]
fn resolve_workspace_install_targets_returns_every_member_when_config_has_no_filter() {
    let metadata = fixture_metadata();
    let targets =
        resolve_workspace_install_targets(&config_with_packages(&[]), &metadata).expect("resolves");
    let names: Vec<&str> = targets.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(names, vec!["pkg-a", "pkg-b"]);
}

#[test]
fn resolve_workspace_install_targets_filters_by_config_packages() {
    let metadata = fixture_metadata();
    let targets = resolve_workspace_install_targets(&config_with_packages(&["pkg-b"]), &metadata)
        .expect("resolves");
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].0, "pkg-b");
    assert_eq!(targets[0].1, PathBuf::from("/workspace/crates/pkg-b"));
}

#[test]
fn resolve_workspace_install_targets_errors_when_the_filter_matches_nothing() {
    let metadata = fixture_metadata();
    let result = resolve_workspace_install_targets(
        &config_with_packages(&["not-a-workspace-member"]),
        &metadata,
    );
    assert!(result.is_err());
}

#[test]
fn resolve_package_path_finds_a_package_by_name() {
    let metadata = fixture_metadata();
    let path = resolve_package_path(&metadata, "pkg-a").unwrap();
    assert_eq!(path, PathBuf::from("/workspace/crates/pkg-a"));
}

#[test]
fn resolve_package_path_errors_for_an_unknown_package() {
    let metadata = fixture_metadata();
    let error = resolve_package_path(&metadata, "no-such-package").unwrap_err();
    assert!(error.to_string().contains("not found in cargo metadata"));
}

#[test]
fn resolve_install_targets_all_flag_uses_the_workspace_resolver() {
    let metadata = fixture_metadata();
    let targets =
        resolve_install_targets(&config_with_packages(&[]), &metadata, None, true).unwrap();
    assert_eq!(targets.len(), 2);
}

#[test]
fn resolve_install_targets_explicit_package_flag_wins_over_config() {
    let metadata = fixture_metadata();
    let targets = resolve_install_targets(
        &config_with_packages(&["pkg-a"]),
        &metadata,
        Some("pkg-b".to_string()),
        false,
    )
    .unwrap();
    assert_eq!(
        targets,
        vec![(
            "pkg-b".to_string(),
            PathBuf::from("/workspace/crates/pkg-b")
        )]
    );
}

#[test]
fn resolve_install_targets_falls_back_to_the_first_configured_package() {
    let metadata = fixture_metadata();
    let targets =
        resolve_install_targets(&config_with_packages(&["pkg-a"]), &metadata, None, false).unwrap();
    assert_eq!(
        targets,
        vec![(
            "pkg-a".to_string(),
            PathBuf::from("/workspace/crates/pkg-a")
        )]
    );
}

#[test]
fn resolve_install_targets_with_nothing_configured_falls_back_to_default_module_name() {
    let metadata = fixture_metadata();
    // No package flag and no config packages, so this falls through to
    // `default_module_name`. Cwd won't canonicalize-match any fixture
    // path, but `pkg-a`'s manifest dir lies under the fixture's
    // `workspace_root` by plain path-prefix (no canonicalization needed
    // for that branch), so it wins the "single workspace member" fallback.
    let targets =
        resolve_install_targets(&config_with_packages(&[]), &metadata, None, false).unwrap();
    assert_eq!(
        targets,
        vec![(
            "pkg-a".to_string(),
            PathBuf::from("/workspace/crates/pkg-a")
        )]
    );
}

#[test]
fn default_module_name_falls_back_to_the_sole_workspace_member() {
    let metadata = json!({
        "workspace_root": "/workspace",
        "packages": [
            {
                "name": "only-member",
                "manifest_path": "/workspace/only-member/Cargo.toml"
            }
        ]
    });
    assert_eq!(default_module_name(&metadata).unwrap(), "only-member");
}

#[test]
fn default_module_name_errors_when_nothing_matches_cwd_or_the_workspace_root() {
    let metadata = json!({
        "workspace_root": "/somewhere-else-entirely",
        "packages": [
            {
                "name": "unrelated",
                "manifest_path": "/workspace/unrelated/Cargo.toml"
            }
        ]
    });
    let error = default_module_name(&metadata).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("Could not determine default package name")
    );
}
