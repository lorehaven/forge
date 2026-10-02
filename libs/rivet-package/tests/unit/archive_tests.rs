use rivet_package::{Limits, Manifest, Package, PackageBuilder, PackageError, sha256_hex};
use std::fmt::Write as _;
use std::io::Cursor;

fn builder() -> PackageBuilder {
    let mut b = PackageBuilder::new(Manifest::new("forge", "0.4.0"));
    b.add_file("overlay.yaml", b"env: forge\n".to_vec())
        .unwrap();
    b.add_file("templates/a.yaml.j2", b"kind: A\n".to_vec())
        .unwrap();
    b
}

fn read(bytes: &[u8]) -> Result<Package, PackageError> {
    Package::read(Cursor::new(bytes), &Limits::default())
}

fn zst(tar_bytes: &[u8]) -> Vec<u8> {
    zstd::stream::encode_all(tar_bytes, 3).unwrap()
}

/// A tar whose single entry has a path `Header::set_path` would have refused.
fn tar_with_raw_path(path: &str, kind: tar::EntryType) -> Vec<u8> {
    let mut header = tar::Header::new_old();
    header.as_old_mut().name[..path.len()].copy_from_slice(path.as_bytes());
    header.set_size(0);
    header.set_mode(0o644);
    header.set_entry_type(kind);
    header.set_cksum();
    let mut out = header.as_bytes().to_vec();
    out.extend_from_slice(&[0u8; 1024]);
    out
}

/// Packs `files` as given, without the builder's guarantees.
fn raw_package(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut b = tar::Builder::new(Vec::new());
    for (name, data) in files {
        let mut h = tar::Header::new_gnu();
        h.set_size(data.len() as u64);
        h.set_mode(0o644);
        h.set_entry_type(tar::EntryType::Regular);
        b.append_data(&mut h, name, *data).unwrap();
    }
    zst(&b.into_inner().unwrap())
}

fn sums(files: &[(&str, &[u8])]) -> String {
    let mut out = String::new();
    for (n, d) in files {
        writeln!(out, "{}  {n}", sha256_hex(d)).unwrap();
    }
    out
}

#[test]
fn round_trips() {
    let bytes = builder().build().unwrap();
    let package = read(&bytes).unwrap();
    assert_eq!(package.manifest.package.name, "forge");
    assert_eq!(package.files["overlay.yaml"], b"env: forge\n");
    assert!(package.files.contains_key("SHA256SUMS"));
    assert!(package.files.contains_key("rivet.toml"));
}

#[test]
fn is_byte_identical_for_identical_input() {
    assert_eq!(builder().build().unwrap(), builder().build().unwrap());
}

#[test]
fn insertion_order_does_not_change_the_bytes() {
    let mut b = PackageBuilder::new(Manifest::new("forge", "0.4.0"));
    b.add_file("templates/a.yaml.j2", b"kind: A\n".to_vec())
        .unwrap();
    b.add_file("overlay.yaml", b"env: forge\n".to_vec())
        .unwrap();
    assert_eq!(b.build().unwrap(), builder().build().unwrap());
}

#[test]
fn builder_requires_the_overlay_and_rejects_reserved_names() {
    let b = PackageBuilder::new(Manifest::new("forge", "0.4.0"));
    assert!(matches!(
        b.build(),
        Err(PackageError::Missing("overlay.yaml"))
    ));

    let mut b = builder();
    assert!(b.add_file("rivet.toml", vec![]).is_err());
    assert!(b.add_file("SHA256SUMS", vec![]).is_err());
    assert!(b.add_file("../x", vec![]).is_err());
    assert!(b.add_file("overlay.yaml", vec![]).is_err());
}

#[test]
fn builder_rejects_an_invalid_manifest() {
    let mut b = PackageBuilder::new(Manifest::new("Bad Name", "0.4.0"));
    b.add_file("overlay.yaml", vec![]).unwrap();
    assert!(matches!(b.build(), Err(PackageError::Manifest(_))));
}

#[test]
fn rejects_non_zstd_input() {
    assert!(matches!(
        read(b"plainly not an archive"),
        Err(PackageError::Archive(_))
    ));
}

#[test]
fn rejects_traversal_and_absolute_paths() {
    for path in ["../evil", "a/../../evil", "/etc/passwd"] {
        let bytes = zst(&tar_with_raw_path(path, tar::EntryType::Regular));
        assert!(
            matches!(read(&bytes), Err(PackageError::UnsafePath(..))),
            "{path}"
        );
    }
}

#[test]
fn rejects_links_and_devices() {
    for kind in [
        tar::EntryType::Symlink,
        tar::EntryType::Link,
        tar::EntryType::Char,
        tar::EntryType::Fifo,
    ] {
        let bytes = zst(&tar_with_raw_path("overlay.yaml", kind));
        assert!(
            matches!(read(&bytes), Err(PackageError::UnsafePath(..))),
            "{kind:?}"
        );
    }
}

#[test]
fn rejects_a_missing_required_file() {
    let manifest = Manifest::new("forge", "0.4.0").to_toml().unwrap();
    let files: Vec<(&str, &[u8])> = vec![("rivet.toml", manifest.as_bytes())];
    let mut all = files.clone();
    let s = sums(&files);
    all.push(("SHA256SUMS", s.as_bytes()));
    assert!(matches!(
        read(&raw_package(&all)),
        Err(PackageError::Missing("overlay.yaml"))
    ));
}

fn tampered(overlay_in_sums: &[u8], overlay_in_archive: &[u8]) -> Vec<u8> {
    let manifest = Manifest::new("forge", "0.4.0").to_toml().unwrap();
    let declared: Vec<(&str, &[u8])> = vec![
        ("overlay.yaml", overlay_in_sums),
        ("rivet.toml", manifest.as_bytes()),
    ];
    let s = sums(&declared);
    raw_package(&[
        ("SHA256SUMS", s.as_bytes()),
        ("overlay.yaml", overlay_in_archive),
        ("rivet.toml", manifest.as_bytes()),
    ])
}

#[test]
fn accepts_a_hand_assembled_package_that_is_consistent() {
    assert!(read(&tampered(b"a", b"a")).is_ok());
}

#[test]
fn rejects_a_file_that_does_not_match_its_checksum() {
    assert!(matches!(
        read(&tampered(b"a", b"b")),
        Err(PackageError::Checksum(_))
    ));
}

#[test]
fn rejects_an_unlisted_file_and_a_phantom_listing() {
    let manifest = Manifest::new("forge", "0.4.0").to_toml().unwrap();

    let listed: Vec<(&str, &[u8])> =
        vec![("overlay.yaml", b"a"), ("rivet.toml", manifest.as_bytes())];
    let s = sums(&listed);
    let unlisted = raw_package(&[
        ("SHA256SUMS", s.as_bytes()),
        ("overlay.yaml", b"a"),
        ("rivet.toml", manifest.as_bytes()),
        ("extra.txt", b"smuggled"),
    ]);
    assert!(matches!(read(&unlisted), Err(PackageError::Checksum(_))));

    let phantom_files: Vec<(&str, &[u8])> = vec![
        ("overlay.yaml", b"a"),
        ("rivet.toml", manifest.as_bytes()),
        ("ghost.txt", b"x"),
    ];
    let s = sums(&phantom_files);
    let phantom = raw_package(&[
        ("SHA256SUMS", s.as_bytes()),
        ("overlay.yaml", b"a"),
        ("rivet.toml", manifest.as_bytes()),
    ]);
    assert!(matches!(read(&phantom), Err(PackageError::Checksum(_))));
}

#[test]
fn rejects_duplicate_entries() {
    let manifest = Manifest::new("forge", "0.4.0").to_toml().unwrap();
    let files: Vec<(&str, &[u8])> =
        vec![("overlay.yaml", b"a"), ("rivet.toml", manifest.as_bytes())];
    let s = sums(&files);
    let bytes = raw_package(&[
        ("SHA256SUMS", s.as_bytes()),
        ("overlay.yaml", b"a"),
        ("overlay.yaml", b"a"),
        ("rivet.toml", manifest.as_bytes()),
    ]);
    assert!(matches!(read(&bytes), Err(PackageError::Archive(_))));
}

#[test]
fn enforces_the_size_and_count_limits() {
    let bytes = builder().build().unwrap();

    let tiny_file = Limits {
        max_file_bytes: 4,
        ..Limits::default()
    };
    assert!(matches!(
        Package::read(Cursor::new(&bytes), &tiny_file),
        Err(PackageError::Limit(_))
    ));

    let tiny_total = Limits {
        max_total_bytes: 40,
        ..Limits::default()
    };
    assert!(matches!(
        Package::read(Cursor::new(&bytes), &tiny_total),
        Err(PackageError::Limit(_))
    ));

    let few = Limits {
        max_entries: 2,
        ..Limits::default()
    };
    assert!(matches!(
        Package::read(Cursor::new(&bytes), &few),
        Err(PackageError::Limit(_))
    ));
}

#[test]
fn a_highly_compressible_bomb_is_stopped_by_the_decoded_limits() {
    let mut b = PackageBuilder::new(Manifest::new("forge", "0.4.0"));
    b.add_file("overlay.yaml", vec![b'x'; 8 * 1024 * 1024])
        .unwrap();
    let bytes = b.build().unwrap();
    assert!(bytes.len() < 4096, "the fixture should compress hard");

    let limits = Limits {
        max_total_bytes: 1024 * 1024,
        ..Limits::default()
    };
    assert!(matches!(
        Package::read(Cursor::new(&bytes), &limits),
        Err(PackageError::Limit(_))
    ));
}

#[test]
fn write_to_materialises_every_file_and_refuses_to_overwrite() {
    let package = read(&builder().build().unwrap()).unwrap();
    let dir = tempfile::tempdir().unwrap();
    package.write_to(dir.path()).unwrap();

    assert_eq!(
        std::fs::read(dir.path().join("templates/a.yaml.j2")).unwrap(),
        b"kind: A\n"
    );
    assert!(package.write_to(dir.path()).is_err());
}

#[test]
fn file_name_follows_the_convention() {
    assert_eq!(
        rivet_package::file_name("forge", "0.4.0+b1"),
        "forge-0.4.0+b1.rivet"
    );
}
