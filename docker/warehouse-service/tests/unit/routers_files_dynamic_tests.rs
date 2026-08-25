//! The dynamic storage filesystem layout - see
//! `docker/warehouse-service/src/routers/files/dynamic.rs`. No database or
//! HTTP request involved: these are pure path-construction rules.

use std::path::Path;
use warehouse_service::routers::files::dynamic::{blob_path, staging_path};

#[test]
fn blob_path_shards_two_levels_deep_under_dot_blobs() {
    let root = Path::new("/data/dynamic");
    let digest = "abcdef0123456789";
    let path = blob_path(root, digest);
    assert_eq!(
        path,
        Path::new("/data/dynamic/.blobs/ab/cd/abcdef0123456789")
    );
}

#[test]
fn blob_path_tolerates_a_digest_shorter_than_the_shard_width() {
    let root = Path::new("/data/dynamic");
    // Too short for even one shard level: no `get(0..2)`, so the digest lands
    // directly under `.blobs`.
    assert_eq!(blob_path(root, ""), Path::new("/data/dynamic/.blobs"));
    // Long enough for one shard level but not two.
    assert_eq!(
        blob_path(root, "ab"),
        Path::new("/data/dynamic/.blobs/ab/ab")
    );
}

#[test]
fn staging_path_calls_never_collide() {
    let root = Path::new("/data/dynamic");
    let a = staging_path(root);
    let b = staging_path(root);
    assert_ne!(a, b);
    assert_eq!(a.parent(), Some(Path::new("/data/dynamic/.tmp")));
}
