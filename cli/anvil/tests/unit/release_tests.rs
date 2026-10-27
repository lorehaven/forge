use anvil::cargo_meta;
use anvil::commands::release::{
    DEFAULT_COMMIT_MESSAGE_TEMPLATE, PackageDependencies, ReleaseKind, ReleasePlanItem,
    build_release_plan, bump_patch, bump_patch_versions, changed_workspace_dependencies,
    collect_package_dependencies, compute_package_layers, ensure_release_tags_absent,
    get_transitive_dependencies, git_show_file_at_tag, is_docker_package, is_release_relevant_file,
    latest_package_tag, nothing_to_release_message, package_changed_since_tag, package_tag_name,
    print_dry_run_plan_with_layers, release_action_label, release_order_graph,
    render_commit_message, resolve_release_targets, resolve_single_package, set_manifest_version,
    should_install_package, tag_exists, workspace_dependencies_table,
};
use anvil::config::{Config, DockerConfig, DockerModuleConfig, InstallConfig};
use quench_serde::json;
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
        needs_release: true,
    }
}

#[quench_test::test]
fn render_commit_message_default_template_matches_the_old_hardcoded_format() {
    let plan = vec![
        plan_item("anvil", ReleaseKind::Cargo, true, false),
        ReleasePlanItem {
            package: "riveter".to_string(),
            to_version: "2.0.0".to_string(),
            ..plan_item("riveter", ReleaseKind::Cargo, true, false)
        },
    ];
    assert_eq!(
        render_commit_message(DEFAULT_COMMIT_MESSAGE_TEMPLATE, &plan),
        "release: bump package versions (anvil v1.0.1, riveter v2.0.0)"
    );
}

#[quench_test::test]
fn render_commit_message_ignores_items_that_were_not_version_bumped() {
    // A first release is `needs_release` but never touches a manifest -
    // nothing for a version-bump commit message to say about it.
    let plan = vec![
        plan_item("anvil", ReleaseKind::Cargo, true, false),
        ReleasePlanItem {
            bump_version: false,
            ..plan_item("brand-new-crate", ReleaseKind::Cargo, false, false)
        },
    ];
    let message = render_commit_message(DEFAULT_COMMIT_MESSAGE_TEMPLATE, &plan);
    assert_eq!(message, "release: bump package versions (anvil v1.0.1)");
    assert!(!message.contains("brand-new-crate"));
}

#[quench_test::test]
fn render_commit_message_supports_a_changelog_placeholder() {
    let plan = vec![
        plan_item("anvil", ReleaseKind::Cargo, true, false),
        ReleasePlanItem {
            package: "riveter".to_string(),
            to_version: "2.0.0".to_string(),
            ..plan_item("riveter", ReleaseKind::Cargo, true, false)
        },
    ];
    let rendered = render_commit_message("release ({count})\n\n{changelog}", &plan);
    assert_eq!(rendered, "release (2)\n\n- anvil v1.0.1\n- riveter v2.0.0");
}

#[quench_test::test]
fn render_commit_message_with_no_bumped_packages_is_empty_summary() {
    let plan: Vec<ReleasePlanItem> = Vec::new();
    assert_eq!(
        render_commit_message(DEFAULT_COMMIT_MESSAGE_TEMPLATE, &plan),
        "release: bump package versions ()"
    );
}

#[quench_test::test]
fn is_release_relevant_file_accepts_known_source_extensions() {
    assert!(is_release_relevant_file("src/main.rs"));
    assert!(is_release_relevant_file("Cargo.toml"));
    assert!(is_release_relevant_file("migrations/0001.sql"));
}

#[quench_test::test]
fn is_release_relevant_file_accepts_assets_that_are_compiled_in() {
    assert!(is_release_relevant_file(
        "src/routers/ui/common/conveyor.css"
    ));
    assert!(is_release_relevant_file("src/routers/ui/common/app.js"));
    assert!(is_release_relevant_file("src/routers/ui/common/page.html"));
    assert!(is_release_relevant_file("src/routers/ui/common/icon.svg"));
}

#[quench_test::test]
fn is_release_relevant_file_rejects_docs_and_extensionless_files() {
    assert!(!is_release_relevant_file("README.md"));
    assert!(!is_release_relevant_file("LICENSE"));
    assert!(!is_release_relevant_file("docs/architecture.md"));
}

#[quench_test::test]
fn bump_patch_increments_only_the_patch_component() {
    assert_eq!(bump_patch("1.2.3").unwrap(), "1.2.4");
    assert_eq!(bump_patch("0.0.0").unwrap(), "0.0.1");
}

#[quench_test::test]
fn bump_patch_rejects_malformed_versions() {
    assert!(bump_patch("1.2").is_err());
    assert!(bump_patch("1.2.3.4").is_err());
    assert!(bump_patch("not-a-version").is_err());
}

#[quench_test::test]
fn package_tag_name_formats_as_package_dash_v_version() {
    assert_eq!(
        package_tag_name("conveyor-service", "0.5.23"),
        "conveyor-service-v0.5.23"
    );
}

#[quench_test::test]
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

#[quench_test::test]
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

#[quench_test::test]
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

#[quench_test::test]
fn get_transitive_dependencies_is_just_the_package_itself_with_no_deps() {
    let graph = HashMap::new();
    let mut visited = HashSet::new();
    get_transitive_dependencies("lonely", &graph, &mut visited);
    assert_eq!(visited, HashSet::from(["lonely".to_string()]));
}

#[quench_test::test]
fn compute_package_layers_places_a_dependency_before_its_dependent() {
    let mut graph = HashMap::new();
    graph.insert("app".to_string(), vec!["lib".to_string()]);
    graph.insert("lib".to_string(), vec![]);

    let packages = vec!["app".to_string(), "lib".to_string()];
    let layers = compute_package_layers(&packages, &graph);

    assert_eq!(layers["lib"], 0);
    assert_eq!(layers["app"], 1);
}

#[quench_test::test]
fn compute_package_layers_ignores_dependencies_outside_the_given_set() {
    let mut graph = HashMap::new();
    // `external` isn't in `packages`, so it must not push `app` to layer 1.
    graph.insert("app".to_string(), vec!["external".to_string()]);

    let packages = vec!["app".to_string()];
    let layers = compute_package_layers(&packages, &graph);
    assert_eq!(layers["app"], 0);
}

#[quench_test::test]
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

#[quench_test::test]
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

#[quench_test::test]
fn workspace_dependencies_table_is_empty_without_a_workspace_dependencies_section() {
    let table = workspace_dependencies_table("[package]\nname = \"x\"").unwrap();
    assert!(table.is_empty());
}

#[quench_test::test]
fn workspace_dependencies_table_errors_on_invalid_toml() {
    assert!(workspace_dependencies_table("not [ valid toml").is_err());
}

#[quench_test::test]
fn nothing_to_release_message_for_all_says_everything_is_up_to_date() {
    assert!(nothing_to_release_message(true).contains("up to date"));
}

#[quench_test::test]
fn nothing_to_release_message_for_a_single_target_names_the_missing_changes() {
    assert!(nothing_to_release_message(false).contains("no changes since its last tag"));
}

#[quench_test::test]
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

#[quench_test::test]
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

#[quench_test::test]
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

#[quench_test::test]
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
            commit_message_template: None,
            gatehouse_url: None,
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

#[quench_test::test]
fn resolve_release_targets_all_errors_when_nothing_is_configured() {
    let metadata =
        json!({ "workspace_root": "/workspace", "workspace_members": [], "packages": [] });
    let error = resolve_release_targets(&Config::default(), &metadata, None, true).unwrap_err();
    assert!(error.to_string().contains("No release packages configured"));
}

#[quench_test::test]
fn resolve_release_targets_all_errors_when_a_configured_package_is_not_a_member() {
    let metadata =
        json!({ "workspace_root": "/workspace", "workspace_members": [], "packages": [] });
    let config = Config {
        release: anvil::config::ReleaseConfig {
            registry: String::new(),
            packages: vec!["ghost-package".to_string()],
            commit_message_template: None,
            gatehouse_url: None,
        },
        ..Config::default()
    };
    let error = resolve_release_targets(&config, &metadata, None, true).unwrap_err();
    assert!(error.to_string().contains("is not a workspace member"));
}

#[quench_test::test]
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

#[quench_test::test]
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

#[quench_test::test]
fn tag_exists_is_false_for_a_tag_that_definitely_does_not_exist() {
    // `git tag` (no `-C`/`current_dir` override) needs cwd inside a git
    // repo to run at all - see `stable_cwd_lock`'s docs.
    let _guard = stable_cwd_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    assert!(!tag_exists("anvil-test-definitely-not-a-real-tag-xyz").unwrap());
}

#[quench_test::test]
fn latest_package_tag_is_none_for_a_package_with_no_matching_tags() {
    let _guard = stable_cwd_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    assert_eq!(
        latest_package_tag("anvil-test-definitely-not-a-real-package-xyz").unwrap(),
        None
    );
}

#[quench_test::test]
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

#[quench_test::test]
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

#[quench_test::test]
fn with_package_version_changes_only_the_package_version_line() {
    use anvil::commands::release::with_package_version;

    let manifest = "# header\n[package]\nname = \"x\"\nversion = \"0.1.0\" # keep me\nedition = \"2024\"\n\n[dependencies]\nserde = { version = \"1\" }\n[dev-dependencies]\nversion = \"9\"\n";
    let updated = with_package_version(manifest, "0.2.0").unwrap();
    assert_eq!(
        updated,
        manifest.replace(
            "version = \"0.1.0\" # keep me",
            "version = \"0.2.0\" # keep me"
        )
    );

    // An indented key, single quotes, CRLF line endings, a workspace-inherited version.
    assert_eq!(
        with_package_version("[package]\r\n  version = '1.0.0'\r\n", "1.0.1").unwrap(),
        "[package]\r\n  version = \"1.0.1\"\r\n"
    );
    assert_eq!(
        with_package_version(
            "[package]\nversion = { workspace = true } # inherited\n",
            "3.0.0"
        )
        .unwrap(),
        "[package]\nversion = \"3.0.0\" # inherited\n"
    );
    // A `version` key of another table, `versions`, and a missing table are not the package version.
    assert!(with_package_version("[dependencies]\nversion = \"1\"\n", "2").is_none());
    assert!(with_package_version("[package]\nname = \"x\"\nversions = 1\n", "2").is_none());
    assert!(with_package_version("", "2").is_none());
    assert_eq!(
        with_package_version("[package] # c\nversion=\"1\"", "2").unwrap(),
        "[package] # c\nversion = \"2\""
    );
}

// -----------------------------------------------------------------
// The functions below shell out to real `git`/`cargo metadata` against this
// very repo. They're all read-only (`git show`/`git diff`/`git tag -l` never
// mutate anything), so it's safe to run them for real rather than faking a
// workspace - `resolve_release_targets`'s tests above use a fictional
// `/workspace` metadata blob because they never touch the filesystem, but
// these functions do, so they need paths that actually exist.

#[quench_test::test]
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

#[quench_test::test]
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

#[quench_test::test]
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

#[quench_test::test]
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

#[quench_test::test]
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

#[quench_test::test]
fn collect_package_dependencies_finds_this_crate_and_its_workspace_members() {
    let _guard = stable_cwd_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let metadata = cargo_meta::cargo_metadata().unwrap();
    let deps = collect_package_dependencies(&metadata).unwrap();
    assert!(deps.contains_key("anvil"));
    // `anvil` depends on `gatehouse-auth` only; every entry should at least
    // have been populated (empty, not missing).
    let anvil_deps = &deps["anvil"];
    assert_eq!(anvil_deps.member_deps, vec!["gatehouse-auth".to_string()]);
}

#[quench_test::test]
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

#[quench_test::test]
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

#[quench_test::test]
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

#[quench_test::test]
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

fn deps(publish: &[&str], dev: &[&str]) -> PackageDependencies {
    let publish_deps: Vec<String> = publish.iter().map(ToString::to_string).collect();
    let mut member_deps = publish_deps.clone();
    member_deps.extend(dev.iter().map(ToString::to_string));
    PackageDependencies {
        member_deps,
        publish_deps,
        external_workspace_deps: HashSet::new(),
    }
}

fn layers_of(graph: &HashMap<String, PackageDependencies>) -> HashMap<String, usize> {
    let mut names: Vec<String> = graph.keys().cloned().collect();
    names.sort();
    compute_package_layers(&names, &release_order_graph(graph))
}

#[quench_test::test]
fn a_cycle_through_dev_dependencies_does_not_scramble_the_layers() {
    // The shape that went wrong: the core crate's tests use a test helper that depends on the core
    // crate, and the packages released after them depend on both.
    let graph = HashMap::from([
        ("core".to_string(), deps(&[], &["test"])),
        ("test".to_string(), deps(&["core"], &[])),
        ("ui".to_string(), deps(&[], &["test"])),
        ("ui-web".to_string(), deps(&["ui"], &["test"])),
        (
            "fullstack".to_string(),
            deps(&["ui-web", "core"], &["test"]),
        ),
    ]);
    let layers = layers_of(&graph);
    assert_eq!(layers["core"], 0);
    assert_eq!(
        layers["test"], 1,
        "the dev edge core -> test closes a cycle and is dropped"
    );
    assert_eq!(
        layers["ui"], 2,
        "a dev-dependency that can be ordered first, is"
    );
    assert_eq!(layers["ui-web"], 3);
    assert_eq!(layers["fullstack"], 4);
    assert!(
        layers.values().all(|l| *l < 10),
        "no runaway layers: {layers:?}"
    );
}

#[quench_test::test]
fn release_order_ignores_a_dev_edge_only_where_it_would_close_a_cycle() {
    let graph = HashMap::from([
        ("a".to_string(), deps(&["b"], &["c"])),
        ("b".to_string(), deps(&[], &["a"])), // a cycle with a -> b: dropped
        ("c".to_string(), deps(&[], &[])),    // a plain dev-dependency: kept
    ]);
    let order = release_order_graph(&graph);
    assert_eq!(order["b"], Vec::<String>::new());
    let mut a = order["a"].clone();
    a.sort();
    assert_eq!(a, vec!["b".to_string(), "c".to_string()]);
}

#[quench_test::test]
fn compute_package_layers_survives_a_cycle_and_does_not_depend_on_input_order() {
    let graph = HashMap::from([
        ("x".to_string(), vec!["y".to_string()]),
        ("y".to_string(), vec!["x".to_string(), "z".to_string()]),
        ("z".to_string(), vec![]),
    ]);
    let forward: Vec<String> = ["x", "y", "z"].map(String::from).to_vec();
    let backward: Vec<String> = ["z", "y", "x"].map(String::from).to_vec();
    let a = compute_package_layers(&forward, &graph);
    let b = compute_package_layers(&backward, &graph);
    assert_eq!(a, b);
    assert_eq!(a["z"], 0);
    assert!(a["x"] < 5 && a["y"] < 5, "{a:?}");
    assert_eq!(compute_package_layers(&[], &graph).len(), 0);
}

#[quench_test::test]
fn collect_package_dependencies_tells_publish_dependencies_from_dev_ones() {
    let dir = quench_core::tmp::TempDir::new().unwrap();
    let manifest = |name: &str, body: &str| {
        let path = dir.path().join(name);
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(
            path.join("Cargo.toml"),
            format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\n{body}"),
        )
        .unwrap();
        path.join("Cargo.toml").display().to_string()
    };
    let app = manifest(
        "app",
        "[dependencies]\nlib = { workspace = true }\nserde = { workspace = true }\n\
         [build-dependencies]\ngen = { workspace = true }\n\
         [dev-dependencies]\nhelper = { workspace = true }\nlib = { workspace = true }\n\
         [target.'cfg(unix)'.dependencies]\nunixlib = { workspace = true }\n\
         [target.'cfg(windows)'.dev-dependencies]\nwinhelper = { workspace = true }\n",
    );
    let others: Vec<String> = ["lib", "gen", "helper", "unixlib", "winhelper"]
        .iter()
        .map(|n| manifest(n, ""))
        .collect();
    let package = |name: &str, path: &str| json!({ "id": name, "name": name, "manifest_path": path, "version": "0.1.0" });
    let mut packages = vec![package("app", &app)];
    for (name, path) in ["lib", "gen", "helper", "unixlib", "winhelper"]
        .iter()
        .zip(&others)
    {
        packages.push(package(name, path));
    }
    let members: Vec<&str> = vec!["app", "lib", "gen", "helper", "unixlib", "winhelper"];
    let metadata = json!({ "workspace_members": members, "packages": packages });

    let found = collect_package_dependencies(&metadata).unwrap();
    let app = &found["app"];
    let mut publish = app.publish_deps.clone();
    publish.sort();
    assert_eq!(
        publish,
        ["gen", "lib", "unixlib"],
        "dependencies, build-dependencies and target.* ones"
    );
    let mut all = app.member_deps.clone();
    all.sort();
    assert_eq!(
        all,
        ["gen", "helper", "lib", "unixlib", "winhelper"],
        "change detection still sees dev ones"
    );
    assert!(app.external_workspace_deps.contains("serde"));
    assert!(!app.publish_deps.contains(&"helper".to_string()));
}
