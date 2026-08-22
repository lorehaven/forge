use anvil::cargo_meta;
use anvil::commands::release::{
    ReleaseKind, ReleasePlanItem, build_release_plan, bump_patch, bump_patch_versions,
    changed_workspace_dependencies, collect_package_dependencies, compute_package_layers,
    ensure_release_plan_non_empty, ensure_release_tags_absent, get_transitive_dependencies,
    git_show_file_at_tag, is_docker_package, is_release_relevant_file, latest_package_tag,
    package_changed_since_tag, package_tag_name, print_dry_run_plan_with_layers,
    release_action_label, resolve_release_targets, resolve_single_package, set_manifest_version,
    should_install_package, tag_exists, workspace_dependencies_table,
};
use anvil::config::{Config, DockerConfig, DockerModuleConfig, InstallConfig};
use serde_json::json;
use std::collections::{HashMap, HashSet};

use crate::support;
use support::stable_cwd_lock;

fn plan_item(
    package: &str,
    kind: ReleaseKind,
    bump_version: bool,
    install_after_publish: bool,
) -> ReleasePlanItem {
    ReleasePlanItem {
        package: package.to_string(),
        from_version: "1.0.0".to_string(),
        to_version: "1.0.1".to_string(),
        kind,
        tag_to_create: format!("{package}-v1.0.1"),
        bump_version,
        install_after_publish,
        layer: 0,
    }
}

#[test]
fn is_release_relevant_file_accepts_known_source_extensions() {
    assert!(is_release_relevant_file("src/main.rs"));
    assert!(is_release_relevant_file("Cargo.toml"));
    assert!(is_release_relevant_file("migrations/0001.sql"));
}

#[test]
fn is_release_relevant_file_rejects_docs_and_extensionless_files() {
    assert!(!is_release_relevant_file("README.md"));
    assert!(!is_release_relevant_file("LICENSE"));
    assert!(!is_release_relevant_file("docs/architecture.md"));
}

#[test]
fn bump_patch_increments_only_the_patch_component() {
    assert_eq!(bump_patch("1.2.3").unwrap(), "1.2.4");
    assert_eq!(bump_patch("0.0.0").unwrap(), "0.0.1");
}

#[test]
fn bump_patch_rejects_malformed_versions() {
    assert!(bump_patch("1.2").is_err());
    assert!(bump_patch("1.2.3.4").is_err());
    assert!(bump_patch("not-a-version").is_err());
}

#[test]
fn package_tag_name_formats_as_package_dash_v_version() {
    assert_eq!(
        package_tag_name("conveyor-service", "0.5.23"),
        "conveyor-service-v0.5.23"
    );
}

#[test]
fn is_docker_package_true_only_when_listed_under_some_module() {
    let config = Config {
        docker: DockerConfig {
            modules: std::iter::once((
                "core".to_string(),
                DockerModuleConfig {
                    packages: vec!["service-a".to_string()],
                    dockerfile: "Dockerfile".to_string(),
                    package_overrides: std::collections::HashMap::new(),
                },
            ))
            .collect(),
            ..DockerConfig::default()
        },
        ..Config::default()
    };
    assert!(is_docker_package(&config, "service-a"));
    assert!(!is_docker_package(&config, "service-b"));
}

#[test]
fn should_install_package_true_only_when_listed_in_install_packages() {
    let config = Config {
        install: InstallConfig {
            packages: vec!["cli-tool".to_string()],
        },
        ..Config::default()
    };
    assert!(should_install_package(&config, "cli-tool"));
    assert!(!should_install_package(&config, "other-tool"));
}

#[test]
fn get_transitive_dependencies_walks_the_whole_chain_and_stops_at_cycles() {
    let mut graph = HashMap::new();
    graph.insert("a".to_string(), vec!["b".to_string()]);
    graph.insert("b".to_string(), vec!["c".to_string()]);
    graph.insert("c".to_string(), vec!["a".to_string()]); // cycle back to a

    let mut visited = HashSet::new();
    get_transitive_dependencies("a", &graph, &mut visited);

    let mut sorted: Vec<_> = visited.into_iter().collect();
    sorted.sort();
    assert_eq!(
        sorted,
        vec!["a".to_string(), "b".to_string(), "c".to_string()]
    );
}

#[test]
fn get_transitive_dependencies_is_just_the_package_itself_with_no_deps() {
    let graph = HashMap::new();
    let mut visited = HashSet::new();
    get_transitive_dependencies("lonely", &graph, &mut visited);
    assert_eq!(visited, HashSet::from(["lonely".to_string()]));
}

#[test]
fn compute_package_layers_places_a_dependency_before_its_dependent() {
    let mut graph = HashMap::new();
    graph.insert("app".to_string(), vec!["lib".to_string()]);
    graph.insert("lib".to_string(), vec![]);

    let packages = vec!["app".to_string(), "lib".to_string()];
    let layers = compute_package_layers(&packages, &graph);

    assert_eq!(layers["lib"], 0);
    assert_eq!(layers["app"], 1);
}

#[test]
fn compute_package_layers_ignores_dependencies_outside_the_given_set() {
    let mut graph = HashMap::new();
    // `external` isn't in `packages`, so it must not push `app` to layer 1.
    graph.insert("app".to_string(), vec!["external".to_string()]);

    let packages = vec!["app".to_string()];
    let layers = compute_package_layers(&packages, &graph);
    assert_eq!(layers["app"], 0);
}

#[test]
fn compute_package_layers_handles_a_three_level_chain() {
    let mut graph = HashMap::new();
    graph.insert("top".to_string(), vec!["mid".to_string()]);
    graph.insert("mid".to_string(), vec!["bottom".to_string()]);
    graph.insert("bottom".to_string(), vec![]);

    let packages = vec!["top".to_string(), "mid".to_string(), "bottom".to_string()];
    let layers = compute_package_layers(&packages, &graph);

    assert_eq!(layers["bottom"], 0);
    assert_eq!(layers["mid"], 1);
    assert_eq!(layers["top"], 2);
}

#[test]
fn workspace_dependencies_table_reads_the_workspace_dependencies_section() {
    let content = r#"
        [workspace.dependencies]
        anyhow = { version = "1.0" }
        serde = { version = "1.0", features = ["derive"] }
    "#;
    let table = workspace_dependencies_table(content).unwrap();
    assert_eq!(table.len(), 2);
    assert!(table.contains_key("anyhow"));
    assert!(table.contains_key("serde"));
}

#[test]
fn workspace_dependencies_table_is_empty_without_a_workspace_dependencies_section() {
    let table = workspace_dependencies_table("[package]\nname = \"x\"").unwrap();
    assert!(table.is_empty());
}

#[test]
fn workspace_dependencies_table_errors_on_invalid_toml() {
    assert!(workspace_dependencies_table("not [ valid toml").is_err());
}

#[test]
fn ensure_release_plan_non_empty_is_ok_when_the_plan_has_items() {
    let plan = vec![plan_item("a", ReleaseKind::Cargo, false, false)];
    assert!(ensure_release_plan_non_empty(false, &plan, false).is_ok());
}

#[test]
fn ensure_release_plan_non_empty_dry_run_with_empty_plan_is_ok() {
    assert!(ensure_release_plan_non_empty(true, &[], true).is_ok());
}

#[test]
fn ensure_release_plan_non_empty_errors_for_all_with_nothing_to_release() {
    let error = ensure_release_plan_non_empty(true, &[], false).unwrap_err();
    assert!(error.to_string().contains("No packages require release"));
}

#[test]
fn ensure_release_plan_non_empty_errors_for_a_single_target_with_no_changes() {
    let error = ensure_release_plan_non_empty(false, &[], false).unwrap_err();
    assert!(error.to_string().contains("no changes since its last tag"));
}

#[test]
fn release_action_label_covers_every_kind_and_install_combination() {
    assert_eq!(
        release_action_label(&plan_item("a", ReleaseKind::Docker, false, false)),
        "docker release"
    );
    assert_eq!(
        release_action_label(&plan_item("a", ReleaseKind::Cargo, false, true)),
        "cargo publish + install"
    );
    assert_eq!(
        release_action_label(&plan_item("a", ReleaseKind::Cargo, false, false)),
        "cargo publish"
    );
}

#[test]
fn resolve_single_package_uses_the_sole_workspace_member_when_cwd_matches_nothing() {
    let metadata = json!({
        "workspace_root": "/workspace",
        "workspace_members": ["only-id"],
        "packages": [
            {
                "id": "only-id",
                "name": "only-member",
                "manifest_path": "/workspace/only-member/Cargo.toml", "version": "1.0.0"
            }
        ]
    });
    assert_eq!(resolve_single_package(&metadata).unwrap(), "only-member");
}

#[test]
fn resolve_single_package_errors_with_multiple_members_and_no_cwd_match() {
    let metadata = json!({
        "workspace_root": "/workspace",
        "workspace_members": ["a-id", "b-id"],
        "packages": [
            { "id": "a-id", "name": "pkg-a", "manifest_path": "/workspace/pkg-a/Cargo.toml", "version": "1.0.0" },
            { "id": "b-id", "name": "pkg-b", "manifest_path": "/workspace/pkg-b/Cargo.toml", "version": "1.0.0" }
        ]
    });
    let error = resolve_single_package(&metadata).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("Could not determine release target")
    );
}

#[test]
fn resolve_release_targets_all_collects_release_and_docker_packages_without_duplicates() {
    let metadata = json!({
        "workspace_root": "/workspace",
        "workspace_members": ["a-id", "b-id"],
        "packages": [
            { "id": "a-id", "name": "pkg-a", "manifest_path": "/workspace/pkg-a/Cargo.toml", "version": "1.0.0" },
            { "id": "b-id", "name": "pkg-b", "manifest_path": "/workspace/pkg-b/Cargo.toml", "version": "1.0.0" }
        ]
    });
    let config = Config {
        release: anvil::config::ReleaseConfig {
            registry: String::new(),
            packages: vec!["pkg-a".to_string()],
        },
        docker: DockerConfig {
            modules: std::iter::once((
                "core".to_string(),
                DockerModuleConfig {
                    // `pkg-a` also appears here - must not be listed twice.
                    packages: vec!["pkg-a".to_string(), "pkg-b".to_string()],
                    dockerfile: "Dockerfile".to_string(),
                    package_overrides: std::collections::HashMap::new(),
                },
            ))
            .collect(),
            ..DockerConfig::default()
        },
        ..Config::default()
    };

    let targets = resolve_release_targets(&config, &metadata, None, true).unwrap();
    assert_eq!(targets, vec!["pkg-a".to_string(), "pkg-b".to_string()]);
}

#[test]
fn resolve_release_targets_all_errors_when_nothing_is_configured() {
    let metadata =
        json!({ "workspace_root": "/workspace", "workspace_members": [], "packages": [] });
    let error = resolve_release_targets(&Config::default(), &metadata, None, true).unwrap_err();
    assert!(error.to_string().contains("No release packages configured"));
}

#[test]
fn resolve_release_targets_all_errors_when_a_configured_package_is_not_a_member() {
    let metadata =
        json!({ "workspace_root": "/workspace", "workspace_members": [], "packages": [] });
    let config = Config {
        release: anvil::config::ReleaseConfig {
            registry: String::new(),
            packages: vec!["ghost-package".to_string()],
        },
        ..Config::default()
    };
    let error = resolve_release_targets(&config, &metadata, None, true).unwrap_err();
    assert!(error.to_string().contains("is not a workspace member"));
}

#[test]
fn resolve_release_targets_explicit_package_must_be_a_workspace_member() {
    let metadata =
        json!({ "workspace_root": "/workspace", "workspace_members": [], "packages": [] });
    let error = resolve_release_targets(
        &Config::default(),
        &metadata,
        Some("ghost".to_string()),
        false,
    )
    .unwrap_err();
    assert!(error.to_string().contains("is not a workspace member"));
}

#[test]
fn resolve_release_targets_explicit_package_that_is_a_member_is_accepted() {
    let metadata = json!({
        "workspace_root": "/workspace",
        "workspace_members": ["a-id"],
        "packages": [{ "id": "a-id", "name": "pkg-a", "manifest_path": "/workspace/pkg-a/Cargo.toml", "version": "1.0.0" }]
    });
    let targets = resolve_release_targets(
        &Config::default(),
        &metadata,
        Some("pkg-a".to_string()),
        false,
    )
    .unwrap();
    assert_eq!(targets, vec!["pkg-a".to_string()]);
}

#[test]
fn tag_exists_is_false_for_a_tag_that_definitely_does_not_exist() {
    // `git tag` (no `-C`/`current_dir` override) needs cwd inside a git
    // repo to run at all - see `stable_cwd_lock`'s docs.
    let _guard = stable_cwd_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    assert!(!tag_exists("anvil-test-definitely-not-a-real-tag-xyz").unwrap());
}

#[test]
fn latest_package_tag_is_none_for_a_package_with_no_matching_tags() {
    let _guard = stable_cwd_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    assert_eq!(
        latest_package_tag("anvil-test-definitely-not-a-real-package-xyz").unwrap(),
        None
    );
}

#[test]
fn ensure_release_tags_absent_passes_when_no_tag_in_the_plan_exists() {
    let _guard = stable_cwd_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let plan = vec![ReleasePlanItem {
        tag_to_create: "anvil-test-definitely-not-a-real-tag-xyz".to_string(),
        ..plan_item("a", ReleaseKind::Cargo, false, false)
    }];
    assert!(ensure_release_tags_absent(&plan).is_ok());
}

#[test]
fn set_manifest_version_rewrites_only_the_version_field() {
    let dir = std::env::temp_dir().join(format!(
        "anvil-release-test-manifest-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("Cargo.toml");
    std::fs::write(
        &path,
        "[package]\nname = \"scratch\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();

    set_manifest_version(&path, "0.2.0").unwrap();

    let rewritten = std::fs::read_to_string(&path).unwrap();
    assert!(rewritten.contains("version = \"0.2.0\""));
    assert!(rewritten.contains("name = \"scratch\""));

    let _ = std::fs::remove_dir_all(&dir);
}

// -----------------------------------------------------------------
// The functions below shell out to real `git`/`cargo metadata` against this
// very repo. They're all read-only (`git show`/`git diff`/`git tag -l` never
// mutate anything), so it's safe to run them for real rather than faking a
// workspace - `resolve_release_targets`'s tests above use a fictional
// `/workspace` metadata blob because they never touch the filesystem, but
// these functions do, so they need paths that actually exist.

#[test]
fn git_show_file_at_tag_reads_a_real_file_at_head() {
    let _guard = stable_cwd_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let metadata = cargo_meta::cargo_metadata().unwrap();
    let root = cargo_meta::workspace_root(&metadata).unwrap();
    let content = git_show_file_at_tag(&root, "HEAD", "Cargo.toml")
        .unwrap()
        .expect("Cargo.toml exists at HEAD");
    assert!(content.contains("[workspace]"));
}

#[test]
fn git_show_file_at_tag_is_none_for_a_path_that_does_not_exist_at_head() {
    let _guard = stable_cwd_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let metadata = cargo_meta::cargo_metadata().unwrap();
    let root = cargo_meta::workspace_root(&metadata).unwrap();
    let missing =
        git_show_file_at_tag(&root, "HEAD", "definitely-not-a-real-file-xyz.toml").unwrap();
    assert_eq!(missing, None);
}

#[test]
fn package_changed_since_tag_is_false_for_head_diffed_against_itself() {
    let _guard = stable_cwd_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let metadata = cargo_meta::cargo_metadata().unwrap();
    let root = cargo_meta::workspace_root(&metadata).unwrap();
    let package_dir = root.join("cli/anvil");
    let changed = package_changed_since_tag(&root, &package_dir, "HEAD").unwrap();
    assert!(!changed);
}

#[test]
fn package_changed_since_tag_errors_when_the_package_dir_is_outside_the_workspace_root() {
    let _guard = stable_cwd_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let metadata = cargo_meta::cargo_metadata().unwrap();
    let root = cargo_meta::workspace_root(&metadata).unwrap();
    let outside = std::env::temp_dir();
    let err = package_changed_since_tag(&root, &outside, "HEAD").unwrap_err();
    assert!(err.to_string().contains("is not under workspace root"));
}

#[test]
fn changed_workspace_dependencies_runs_against_the_real_workspace_without_error() {
    // Diffs `HEAD`'s `Cargo.toml` against the working tree's - this repo can
    // legitimately have uncommitted `[workspace.dependencies]` edits at test
    // time, so this only asserts the comparison itself succeeds, not that
    // the resulting set is empty.
    let _guard = stable_cwd_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let metadata = cargo_meta::cargo_metadata().unwrap();
    let root = cargo_meta::workspace_root(&metadata).unwrap();
    changed_workspace_dependencies(&root, "HEAD").unwrap();
}

#[test]
fn collect_package_dependencies_finds_this_crate_and_its_workspace_members() {
    let _guard = stable_cwd_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let metadata = cargo_meta::cargo_metadata().unwrap();
    let deps = collect_package_dependencies(&metadata).unwrap();
    assert!(deps.contains_key("anvil"));
    // `anvil` depends on nothing else in the workspace, but every entry
    // should at least have been populated (empty, not missing).
    let anvil_deps = &deps["anvil"];
    assert!(anvil_deps.member_deps.is_empty());
}

#[test]
fn build_release_plan_against_the_real_workspace_never_panics_and_is_read_only() {
    let _guard = stable_cwd_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let metadata = cargo_meta::cargo_metadata().unwrap();
    let config = Config::default();
    // Each of these has no in-workspace dependents, so none can cascade into
    // planning a release for the rest of the workspace. Trying several
    // real packages (rather than just one) covers more of the plan-building
    // branches: whichever combination of "never tagged" / "tagged with no
    // changes since" / "tagged with changes since" each happens to be in
    // right now, without this test needing to know or assume which.
    for package in [
        "forge-toolbox",
        "conveyor-pipeline",
        "foundry-service",
        "workbench-service",
    ] {
        let plan = build_release_plan(&config, &metadata, &[package.to_string()]).unwrap();
        for item in &plan {
            assert!(!item.package.is_empty());
            assert!(!item.tag_to_create.is_empty());
        }
    }
}

#[test]
fn print_dry_run_plan_with_layers_does_not_panic_for_an_empty_or_populated_plan() {
    print_dry_run_plan_with_layers(&[]);
    print_dry_run_plan_with_layers(&[
        plan_item("a", ReleaseKind::Cargo, true, false),
        ReleasePlanItem {
            layer: 1,
            ..plan_item("b", ReleaseKind::Docker, false, true)
        },
    ]);
}

#[test]
fn bump_patch_versions_rewrites_only_the_manifests_flagged_for_a_version_bump() {
    let dir = std::env::temp_dir().join(format!(
        "anvil-release-test-bump-{}-{}",
        std::process::id(),
        line!()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let manifest_path = dir.join("Cargo.toml");
    std::fs::write(
        &manifest_path,
        "[package]\nname = \"scratch\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();

    let metadata = json!({
        "workspace_root": dir.to_string_lossy(),
        "workspace_members": ["scratch-id"],
        "packages": [{
            "id": "scratch-id",
            "name": "scratch",
            "manifest_path": manifest_path.to_string_lossy(),
            "version": "0.1.0",
        }],
    });

    let plan = vec![ReleasePlanItem {
        to_version: "0.2.0".to_string(),
        bump_version: true,
        ..plan_item("scratch", ReleaseKind::Cargo, true, false)
    }];

    let touched = bump_patch_versions(&metadata, &plan).unwrap();
    assert_eq!(touched, vec![manifest_path.clone()]);

    let rewritten = std::fs::read_to_string(&manifest_path).unwrap();
    assert!(rewritten.contains("version = \"0.2.0\""));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn bump_patch_versions_skips_items_not_flagged_for_a_bump() {
    let metadata = json!({
        "workspace_root": "/workspace",
        "workspace_members": ["a-id"],
        "packages": [{ "id": "a-id", "name": "pkg-a", "manifest_path": "/workspace/pkg-a/Cargo.toml", "version": "1.0.0" }]
    });
    let plan = vec![plan_item("pkg-a", ReleaseKind::Cargo, false, false)];
    let touched = bump_patch_versions(&metadata, &plan).unwrap();
    assert!(touched.is_empty());
}
