use warehouse_core::tree::{
    Selection, active_crate_version, artifact_tree, file_tree, last_segment, normalize_path,
    parent_dir, pick_artifact, pick_rivet, repo_tree, rivet_tree,
};
use warehouse_core::{ArtifactEntry, BrowseFile, CrateVersion, RivetEntry};

fn strings(names: &[&str]) -> Vec<String> {
    names.iter().map(ToString::to_string).collect()
}

#[quench_test::test]
fn repositories_nest_by_their_slashes() {
    let tree = repo_tree(&strings(&["app", "team/api", "team/web", "team"]));
    assert_eq!(tree.children.len(), 2);
    let team = &tree.children["team"];
    assert_eq!(team.repo.as_deref(), Some("team"));
    assert_eq!(team.children.len(), 2);
    assert!(tree.holds("team/api"));
    assert!(team.holds("team/web"));
    assert!(!tree.children["app"].holds("team/web"));
    assert!(!tree.holds("nope"));
}

fn file(path: &str, size: i64) -> BrowseFile {
    BrowseFile {
        path: path.into(),
        size: Some(size),
    }
}

#[quench_test::test]
fn files_become_a_tree_that_resolves_a_path() {
    let tree = file_tree(&[file("a/b.txt", 3), file("a/c/d.bin", 4), file("top.md", 5)]);
    assert_eq!(tree.resolve(""), Selection::Dir(String::new()));
    assert_eq!(tree.resolve("a"), Selection::Dir("a".into()));
    assert_eq!(
        tree.resolve("a/b.txt"),
        Selection::File {
            path: "a/b.txt".into(),
            size: Some(3)
        }
    );
    assert_eq!(tree.resolve("a/zzz"), Selection::Missing("a/zzz".into()));
    assert_eq!(tree.resolve("a/zzz").path(), "a/zzz");
    let root: Vec<_> = tree
        .sorted_children()
        .into_iter()
        .map(|(n, _)| n.as_str())
        .collect();
    assert_eq!(root, ["a", "top.md"], "directories first");
    assert!(tree.has_dirs());
    assert!(!tree.node_at("a/c").unwrap().has_dirs());
    assert!(tree.node_at("top.md").unwrap().is_leaf());
}

#[quench_test::test]
fn paths_are_tidied() {
    assert_eq!(normalize_path("/a/./b/../c//"), "a/b/c");
    assert_eq!(parent_dir("a/b/c.txt"), "a/b");
    assert_eq!(parent_dir("c.txt"), "");
    assert_eq!(last_segment("a/b/c.txt"), "c.txt");
    assert_eq!(last_segment("c.txt"), "c.txt");
}

fn version(vers: &str, yanked: bool) -> CrateVersion {
    CrateVersion {
        vers: vers.into(),
        yanked,
        cksum: "c".into(),
        rust_version: None,
        links: None,
        features: vec![],
        deps: vec![],
    }
}

#[quench_test::test]
fn the_open_crate_version_is_the_newest_that_is_not_yanked() {
    let versions = [
        version("0.1.0", false),
        version("0.2.0", false),
        version("0.3.0", true),
    ];
    assert_eq!(active_crate_version(&versions, None), Some(1));
    assert_eq!(active_crate_version(&versions, Some("0.3.0")), Some(2));
    assert_eq!(active_crate_version(&versions, Some("9.9.9")), Some(1));
    let all_yanked = [version("1.0.0", true), version("1.1.0", true)];
    assert_eq!(active_crate_version(&all_yanked, None), Some(1));
    assert_eq!(active_crate_version(&[], None), None);
}

fn artifact(program: &str, platform: &str, code: i64) -> ArtifactEntry {
    ArtifactEntry {
        program: program.into(),
        platform: platform.into(),
        format: "apk".into(),
        version_code: code,
        version_name: format!("{code}.0"),
        filename: "f".into(),
        size_bytes: 1,
        sha256: "s".into(),
        uploaded_by: "u".into(),
        yanked: false,
        arch: None,
        label: None,
        min_sdk_version: None,
        target_sdk_version: None,
        permissions: vec![],
    }
}

#[quench_test::test]
fn artifacts_group_by_program_and_platform_newest_first() {
    let tree = artifact_tree(&[
        artifact("app", "linux", 1),
        artifact("app", "android", 1),
        artifact("app", "android", 3),
        artifact("zed", "windows", 2),
    ]);
    let pick = pick_artifact(&tree, Some("app"), None, None);
    assert_eq!(pick.platform, Some("android"), "the first platform");
    assert_eq!(pick.selected.unwrap().version_code, 3, "the newest");
    let pick = pick_artifact(&tree, Some("app"), Some("android"), Some(1));
    assert_eq!(pick.selected.unwrap().version_code, 1);
    let pick = pick_artifact(&tree, Some("app"), Some("macos"), Some(99));
    assert_eq!(pick.platform, Some("android"));
    assert_eq!(pick.selected.unwrap().version_code, 3);
    let pick = pick_artifact(&tree, Some("nope"), Some("linux"), None);
    assert_eq!(
        (pick.program, pick.platform, pick.selected),
        (None, None, None)
    );
    assert_eq!(pick_artifact(&tree, None, None, None).selected, None);
}

fn rivet(name: &str, version: &str, yanked: bool) -> RivetEntry {
    RivetEntry {
        name: name.into(),
        version: version.into(),
        size_bytes: 1,
        sha256: "s".into(),
        uploaded_by: "u".into(),
        yanked,
        published: "p".into(),
        description: None,
        namespace: None,
        requires_riveter: None,
        requires_packages: vec![],
        meta: vec![],
    }
}

#[quench_test::test]
fn the_open_rivet_is_what_latest_would_give() {
    let tree = rivet_tree(&[
        rivet("b", "2.0.0", true),
        rivet("b", "1.0.0", false),
        rivet("a", "1.0.0", false),
    ]);
    assert_eq!(tree.keys().collect::<Vec<_>>(), ["a", "b"]);
    let versions = &tree["b"];
    assert_eq!(pick_rivet(versions, None).unwrap().version, "1.0.0");
    assert_eq!(
        pick_rivet(versions, Some("2.0.0")).unwrap().version,
        "2.0.0"
    );
    assert_eq!(pick_rivet(versions, Some("9")).unwrap().version, "1.0.0");
    let yanked = [rivet("c", "1.0.0", true)];
    assert_eq!(pick_rivet(&yanked, None).unwrap().version, "1.0.0");
    assert!(pick_rivet(&[], None).is_none());
}
