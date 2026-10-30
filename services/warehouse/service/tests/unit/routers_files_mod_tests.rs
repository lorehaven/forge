use std::path::PathBuf;
use warehouse_service::routers::files::{
    PathError, Storage, confined, parse_storages, relative, resolve,
};

// ---------------------------------------------------------------------
// parse_storages
// ---------------------------------------------------------------------

#[quench_test::test]
fn parse_storages_reads_every_well_formed_entry() {
    let storages = parse_storages("artifacts=/storage/artifacts;media=/mnt/media");
    assert_eq!(
        storages,
        vec![
            Storage {
                name: "artifacts".to_string(),
                root: PathBuf::from("/storage/artifacts")
            },
            Storage {
                name: "media".to_string(),
                root: PathBuf::from("/mnt/media")
            },
        ]
    );
}

#[quench_test::test]
fn parse_storages_is_empty_for_a_blank_string() {
    assert!(parse_storages("").is_empty());
    assert!(parse_storages("   ").is_empty());
}

#[quench_test::test]
fn parse_storages_drops_entries_with_no_equals_sign() {
    assert!(parse_storages("not-a-pair").is_empty());
}

#[quench_test::test]
fn parse_storages_drops_entries_with_an_empty_name_or_path() {
    assert!(parse_storages("=/root").is_empty());
    assert!(parse_storages("name=").is_empty());
}

#[quench_test::test]
fn parse_storages_drops_names_with_disallowed_characters() {
    assert!(parse_storages("has space=/root").is_empty());
    assert!(parse_storages("has/slash=/root").is_empty());
}

#[quench_test::test]
fn parse_storages_keeps_the_first_of_a_duplicate_name() {
    let storages = parse_storages("dup=/one;dup=/two");
    assert_eq!(storages.len(), 1);
    assert_eq!(storages[0].root, PathBuf::from("/one"));
}

#[quench_test::test]
fn parse_storages_trims_whitespace_around_names_and_paths() {
    let storages = parse_storages(" artifacts = /storage/artifacts ; media=/mnt/media ");
    assert_eq!(storages.len(), 2);
    assert_eq!(storages[0].name, "artifacts");
}

// ---------------------------------------------------------------------
// relative
// ---------------------------------------------------------------------

#[quench_test::test]
fn relative_accepts_a_plain_nested_path() {
    assert_eq!(relative("a/b/c").unwrap(), PathBuf::from("a/b/c"));
}

#[quench_test::test]
fn relative_rejects_an_empty_or_blank_path() {
    assert_eq!(relative(""), Err(PathError::Empty));
    assert_eq!(relative("   "), Err(PathError::Empty));
}

#[quench_test::test]
fn relative_rejects_control_bytes() {
    assert_eq!(relative("a\0b"), Err(PathError::Invalid));
    assert_eq!(relative("a\x7fb"), Err(PathError::Invalid));
}

#[quench_test::test]
fn relative_rejects_parent_dir_traversal() {
    assert_eq!(relative("../etc/passwd"), Err(PathError::Traversal));
    assert_eq!(relative("a/../../b"), Err(PathError::Traversal));
}

#[quench_test::test]
fn relative_rejects_absolute_paths() {
    assert_eq!(relative("/etc/passwd"), Err(PathError::Absolute));
}

#[quench_test::test]
fn relative_drops_current_dir_components() {
    assert_eq!(relative("./a/./b"), Ok(PathBuf::from("a/b")));
}

#[quench_test::test]
fn relative_rejects_a_path_that_is_only_current_dir_components() {
    assert_eq!(relative("./."), Err(PathError::Empty));
}

// ---------------------------------------------------------------------
// resolve
// ---------------------------------------------------------------------

#[quench_test::test]
fn resolve_joins_the_relative_path_onto_the_storage_root() {
    let storage = Storage {
        name: "s".to_string(),
        root: PathBuf::from("/storage/s"),
    };
    assert_eq!(
        resolve(&storage, "a/b").unwrap(),
        PathBuf::from("/storage/s/a/b")
    );
}

#[quench_test::test]
fn resolve_propagates_a_relative_path_rejection() {
    let storage = Storage {
        name: "s".to_string(),
        root: PathBuf::from("/storage/s"),
    };
    assert_eq!(resolve(&storage, "../x"), Err(PathError::Traversal));
}

// ---------------------------------------------------------------------
// confined
// ---------------------------------------------------------------------

#[quench_test::test]
async fn confined_is_true_for_a_path_that_stays_inside_the_root() {
    let dir = quench_core::tmp::TempDir::new().unwrap();
    let target = dir.path().join("a").join("b.txt");
    tokio::fs::create_dir_all(target.parent().unwrap())
        .await
        .unwrap();
    tokio::fs::write(&target, b"hi").await.unwrap();

    assert!(confined(dir.path(), &target).await);
}

#[quench_test::test]
async fn confined_is_true_for_a_not_yet_existing_target_whose_parent_exists() {
    let dir = quench_core::tmp::TempDir::new().unwrap();
    let target = dir.path().join("does-not-exist-yet.txt");
    assert!(confined(dir.path(), &target).await);
}

#[quench_test::test]
async fn confined_is_false_when_a_symlink_escapes_the_root() {
    let dir = quench_core::tmp::TempDir::new().unwrap();
    let outside = quench_core::tmp::TempDir::new().unwrap();
    let link = dir.path().join("escape");
    #[cfg(unix)]
    std::os::unix::fs::symlink(outside.path(), &link).unwrap();

    assert!(!confined(dir.path(), &link).await);
}

#[quench_test::test]
async fn confined_is_false_when_the_storage_root_itself_does_not_exist() {
    let missing_root = std::path::Path::new("/does/not/exist/at/all");
    let target = missing_root.join("a.txt");
    assert!(!confined(missing_root, &target).await);
}

// ---------------------------------------------------------------------
// PathError
// ---------------------------------------------------------------------

#[quench_test::test]
fn path_error_messages_are_distinct_and_non_empty() {
    let messages = [
        PathError::Empty.message(),
        PathError::Absolute.message(),
        PathError::Traversal.message(),
        PathError::Invalid.message(),
    ];
    for message in messages {
        assert!(!message.is_empty());
    }
}
