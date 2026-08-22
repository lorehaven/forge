use crate::support;

use support::WithCratesStorageRoot as WithStorageRoot;
use warehouse_service::routers::admin::crates::gc::garbage_collect;

fn write_crate_file(storage: &WithStorageRoot, name: &str, version: &str) {
    let dir = storage.dir.path().join(name).join(version);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("{name}-{version}.crate")), b"data").unwrap();
}

fn write_index(storage: &WithStorageRoot, name: &str, entries: &[(&str, bool)]) {
    // Mirrors `index_prefix`'s convention closely enough for a short name.
    let prefix = match name.len() {
        1 => "1".to_string(),
        2 => "2".to_string(),
        3 => format!("3/{}", &name[..1]),
        _ => format!("{}/{}", &name[..2], &name[2..4]),
    };
    let dir = storage.dir.path().join("index").join(prefix);
    std::fs::create_dir_all(&dir).unwrap();
    let mut content = String::new();
    for (version, yanked) in entries {
        content.push_str(&format!(
            r#"{{"name":"{name}","vers":"{version}","yanked":{yanked}}}"#
        ));
        content.push('\n');
    }
    std::fs::write(dir.join(name), content).unwrap();
}

#[tokio::test]
async fn garbage_collect_deletes_yanked_and_orphaned_tarballs_but_keeps_indexed_ones() {
    let storage = WithStorageRoot::new();

    // Kept: indexed and not yanked.
    write_crate_file(&storage, "keep-crate", "1.0.0");
    write_index(&storage, "keep-crate", &[("1.0.0", false)]);

    // Deleted: indexed but yanked.
    write_crate_file(&storage, "yanked-crate", "1.0.0");
    write_index(&storage, "yanked-crate", &[("1.0.0", true)]);

    // Deleted: no index entry at all (orphan tarball); its directory should
    // also be removed once empty.
    write_crate_file(&storage, "orphan-crate", "1.0.0");

    let report = garbage_collect().await.expect("gc succeeds");

    assert_eq!(report.deleted_crates, 2);
    assert_eq!(report.kept_crates, 1);
    assert!(report.removed_empty_dirs >= 2);

    assert!(
        storage
            .dir
            .path()
            .join("keep-crate/1.0.0/keep-crate-1.0.0.crate")
            .exists()
    );
    assert!(
        !storage
            .dir
            .path()
            .join("yanked-crate/1.0.0/yanked-crate-1.0.0.crate")
            .exists()
    );
    assert!(
        !storage
            .dir
            .path()
            .join("orphan-crate/1.0.0/orphan-crate-1.0.0.crate")
            .exists()
    );
}

#[tokio::test]
async fn garbage_collect_repairs_an_index_entry_whose_tarball_is_missing() {
    let storage = WithStorageRoot::new();

    // Index references a version whose tarball was never written (or was
    // already removed) - the entry should be dropped from the index. The GC
    // only visits crates it finds a top-level directory for, so create an
    // (otherwise empty) one even though there's no version subdirectory.
    std::fs::create_dir_all(storage.dir.path().join("ghost-crate")).unwrap();
    write_index(&storage, "ghost-crate", &[("9.9.9", false)]);

    let report = garbage_collect().await.expect("gc succeeds");
    assert_eq!(report.removed_index_entries, 1);
}

#[tokio::test]
async fn garbage_collect_deletes_an_orphaned_owners_json_with_no_index_file() {
    let storage = WithStorageRoot::new();

    let dir = storage.dir.path().join("no-index-crate");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("owners.json"), b"[]").unwrap();

    let report = garbage_collect().await.expect("gc succeeds");
    assert_eq!(report.deleted_owner_files, 1);
    assert!(!dir.join("owners.json").exists());
}

#[tokio::test]
async fn garbage_collect_on_an_empty_storage_root_reports_nothing() {
    let storage = WithStorageRoot::new();
    std::fs::create_dir_all(storage.dir.path()).unwrap();

    let report = garbage_collect().await.expect("gc succeeds");
    assert_eq!(report.deleted_crates, 0);
    assert_eq!(report.kept_crates, 0);
    assert_eq!(report.removed_index_entries, 0);
    assert_eq!(report.deleted_owner_files, 0);
}
