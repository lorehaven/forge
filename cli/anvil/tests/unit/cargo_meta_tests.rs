use anvil::cargo_meta::WorkspacePackage;
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
