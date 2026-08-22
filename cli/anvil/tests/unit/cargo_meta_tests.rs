use anvil::cargo_meta::{
    WorkspacePackage, cargo_metadata, resolve_package, workspace_packages, workspace_root,
};
use serde_json::json;
use std::path::{Path, PathBuf};

fn package(publish: Option<Vec<String>>) -> WorkspacePackage {
    WorkspacePackage {
        name: "example".to_string(),
        dir: PathBuf::from("/workspace/example"),
        manifest: PathBuf::from("/workspace/example/Cargo.toml"),
        version: "0.1.0".to_string(),
        publish,
    }
}

#[test]
fn publish_disabled_is_true_only_for_an_explicit_empty_list() {
    assert!(package(Some(Vec::new())).publish_disabled());
}

#[test]
fn publish_disabled_is_false_when_unrestricted() {
    assert!(!package(None).publish_disabled());
}

#[test]
fn publish_disabled_is_false_when_restricted_to_specific_registries() {
    assert!(!package(Some(vec!["ennor".to_string()])).publish_disabled());
}

#[test]
fn relative_dir_strips_the_workspace_root_regardless_of_nesting_depth() {
    // Forge's own convention: packages nested under docker/<service>/.
    let nested = package(None);
    assert_eq!(
        nested.relative_dir(Path::new("/workspace")).unwrap(),
        PathBuf::from("example")
    );

    // A package living directly at the workspace root (palantir's `server`,
    // for instance) - no module subdirectory assumed.
    let root_level = WorkspacePackage {
        dir: PathBuf::from("/workspace/server"),
        manifest: PathBuf::from("/workspace/server/Cargo.toml"),
        ..package(None)
    };
    assert_eq!(
        root_level.relative_dir(Path::new("/workspace")).unwrap(),
        PathBuf::from("server")
    );
}

#[test]
fn relative_dir_errors_when_the_package_is_not_under_the_given_root() {
    let pkg = package(None);
    assert!(pkg.relative_dir(Path::new("/somewhere/else")).is_err());
}

#[test]
fn cargo_metadata_runs_real_cargo_and_returns_this_workspace() {
    let metadata = cargo_metadata().expect("cargo metadata succeeds");
    assert!(metadata["packages"].is_array());
    assert!(metadata["workspace_root"].is_string());
}

#[test]
fn workspace_root_reads_the_field_as_a_path() {
    let metadata = json!({ "workspace_root": "/some/workspace" });
    assert_eq!(
        workspace_root(&metadata).unwrap(),
        PathBuf::from("/some/workspace")
    );
}

#[test]
fn workspace_root_errors_when_the_field_is_missing() {
    let metadata = json!({});
    assert!(workspace_root(&metadata).is_err());
}

fn fixture_metadata() -> serde_json::Value {
    json!({
        "workspace_root": "/workspace",
        "workspace_members": ["pkg-a-id", "pkg-b-id"],
        "packages": [
            {
                "id": "pkg-a-id",
                "name": "pkg-a",
                "version": "1.0.0",
                "manifest_path": "/workspace/crates/pkg-a/Cargo.toml",
                "publish": null
            },
            {
                "id": "pkg-b-id",
                "name": "pkg-b",
                "version": "2.0.0",
                "manifest_path": "/workspace/crates/pkg-b/Cargo.toml",
                "publish": []
            },
            {
                "id": "not-a-workspace-member-id",
                "name": "external-dep",
                "version": "9.9.9",
                "manifest_path": "/registry/external-dep/Cargo.toml",
                "publish": null
            }
        ]
    })
}

#[test]
fn workspace_packages_only_includes_workspace_members_and_reads_their_fields() {
    let metadata = fixture_metadata();
    let packages = workspace_packages(&metadata).expect("resolves");

    assert_eq!(packages.len(), 2);

    let pkg_a = packages.iter().find(|p| p.name == "pkg-a").unwrap();
    assert_eq!(pkg_a.version, "1.0.0");
    assert_eq!(pkg_a.dir, PathBuf::from("/workspace/crates/pkg-a"));
    assert_eq!(
        pkg_a.manifest,
        PathBuf::from("/workspace/crates/pkg-a/Cargo.toml")
    );
    assert_eq!(pkg_a.publish, None);
    assert!(!pkg_a.publish_disabled());

    let pkg_b = packages.iter().find(|p| p.name == "pkg-b").unwrap();
    assert_eq!(pkg_b.publish, Some(Vec::new()));
    assert!(pkg_b.publish_disabled());
}

#[test]
fn workspace_packages_errors_when_packages_field_is_missing() {
    let metadata = json!({ "workspace_members": [] });
    assert!(workspace_packages(&metadata).is_err());
}

#[test]
fn workspace_packages_is_empty_when_workspace_members_is_missing() {
    let mut metadata = fixture_metadata();
    metadata
        .as_object_mut()
        .unwrap()
        .remove("workspace_members");
    let packages = workspace_packages(&metadata).expect("resolves");
    assert!(packages.is_empty());
}

#[test]
fn resolve_package_finds_this_very_crate_in_the_real_workspace() {
    let pkg = resolve_package("anvil").expect("anvil is in this workspace");
    assert_eq!(pkg.name, "anvil");
    assert!(pkg.manifest.ends_with("Cargo.toml"));
}

#[test]
fn resolve_package_errors_for_a_name_not_in_the_workspace() {
    let error = resolve_package("definitely-not-a-real-crate-name-xyz").unwrap_err();
    assert!(
        error
            .to_string()
            .contains("not found in workspace metadata")
    );
}
