//! The second half of the check, against a real filesystem.
//!
//! `relative` guarantees a path *spells* something inside the storage.
//! `confined` answers whether it *is* - which is a different question the
//! moment a symlink is involved, and symlinks get into a storage the same way
//! anything else does.

use warehouse_service::routers::files::confined;

/// A storage root and somewhere outside it, both real directories.
fn estate() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let temp = tempfile::tempdir().expect("temp dir");
    let root = temp.path().join("storage");
    let outside = temp.path().join("outside");
    std::fs::create_dir_all(&root).expect("storage root");
    std::fs::create_dir_all(&outside).expect("outside");
    (temp, root, outside)
}

#[tokio::test]
async fn a_file_inside_the_storage_is_confined() {
    let (_temp, root, _outside) = estate();
    std::fs::write(root.join("thing"), b"content").expect("write");

    assert!(confined(&root, &root.join("thing")).await);
}

#[tokio::test]
async fn a_path_that_does_not_exist_yet_is_confined_by_its_parent() {
    let (_temp, root, _outside) = estate();

    // The upload case: nothing is there yet, and the answer has to come from
    // the deepest ancestor that does exist.
    assert!(confined(&root, &root.join("new/deep/file")).await);
}

#[tokio::test]
async fn the_storage_root_itself_is_confined() {
    let (_temp, root, _outside) = estate();

    assert!(confined(&root, &root).await);
}

#[tokio::test]
async fn a_symlinked_file_pointing_outside_is_refused() {
    let (_temp, root, outside) = estate();
    let secret = outside.join("secret");
    std::fs::write(&secret, b"not yours").expect("write");

    #[cfg(unix)]
    std::os::unix::fs::symlink(&secret, root.join("innocent")).expect("symlink");

    // Spelled entirely inside the storage; resolves entirely outside it.
    assert!(!confined(&root, &root.join("innocent")).await);
}

#[tokio::test]
async fn a_symlinked_directory_pointing_outside_is_refused() {
    let (_temp, root, outside) = estate();

    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, root.join("escape")).expect("symlink");

    // The write case: the file does not exist, and the parent that does is a
    // link out of the storage. Creating it would put a caller's bytes outside.
    assert!(!confined(&root, &root.join("escape/new-file")).await);
}

#[tokio::test]
async fn a_symlink_that_stays_inside_is_still_confined() {
    let (_temp, root, _outside) = estate();
    let real = root.join("real");
    std::fs::create_dir_all(&real).expect("real dir");

    #[cfg(unix)]
    std::os::unix::fs::symlink(&real, root.join("alias")).expect("symlink");

    // Refusing every symlink would be the easy rule and the wrong one: this
    // one never leaves the storage.
    assert!(confined(&root, &root.join("alias/file")).await);
}

#[tokio::test]
async fn a_missing_storage_root_confines_nothing() {
    let (_temp, root, _outside) = estate();
    let absent = root.join("not-created");

    // Nothing to canonicalise against, so there is no basis for saying a write
    // would land inside it.
    assert!(!confined(&absent, &absent.join("file")).await);
}

#[tokio::test]
async fn a_sibling_directory_sharing_a_name_prefix_is_refused() {
    let (_temp, root, _outside) = estate();

    // `/tmp/x/storage-elsewhere` starts with `/tmp/x/storage` as a *string*.
    // `starts_with` on a Path compares components, which is why this is
    // refused - and why the check must not be done on strings.
    let sibling = root.with_file_name("storage-elsewhere");
    std::fs::create_dir_all(&sibling).expect("sibling");

    assert!(!confined(&root, &sibling.join("file")).await);
}
