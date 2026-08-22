use crate::support;

use std::collections::HashMap;
use support::WithCratesStorageRoot as WithStorageRoot;
use warehouse_service::routers::crates::index_file_path;
use warehouse_service::routers::ui::pages::crates::storage::{
    IndexRecord, list_crates, list_versions,
};

fn write_index_line(name: &str, record: &IndexRecord) {
    let path = index_file_path(name).expect("valid name");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut content = std::fs::read_to_string(&path).unwrap_or_default();
    content.push_str(&serde_json::to_string(record).unwrap());
    content.push('\n');
    std::fs::write(&path, content).unwrap();
}

fn sample_record(name: &str, version: &str) -> IndexRecord {
    IndexRecord {
        name: name.to_string(),
        vers: version.to_string(),
        deps: vec![],
        cksum: "abc123".to_string(),
        features: HashMap::new(),
        features2: None,
        yanked: false,
        links: None,
        rust_version: None,
        v: 1,
    }
}

#[test]
fn list_crates_is_empty_when_the_index_directory_does_not_exist() {
    let _storage = WithStorageRoot::new();
    assert!(list_crates().is_empty());
}

#[test]
fn list_crates_finds_every_crate_with_an_index_file_sorted_by_name() {
    let _storage = WithStorageRoot::new();
    write_index_line("zeta", &sample_record("zeta", "1.0.0"));
    write_index_line("alpha", &sample_record("alpha", "1.0.0"));

    assert_eq!(list_crates(), vec!["alpha".to_string(), "zeta".to_string()]);
}

#[test]
fn list_versions_is_empty_for_an_unknown_crate() {
    let _storage = WithStorageRoot::new();
    assert!(list_versions("does-not-exist").is_empty());
}

#[test]
fn list_versions_reads_every_line_of_the_index_file_in_order() {
    let _storage = WithStorageRoot::new();
    write_index_line("my-crate", &sample_record("my-crate", "1.0.0"));
    write_index_line("my-crate", &sample_record("my-crate", "1.1.0"));

    let versions = list_versions("my-crate");
    assert_eq!(versions.len(), 2);
    assert_eq!(versions[0].vers, "1.0.0");
    assert_eq!(versions[1].vers, "1.1.0");
}

#[test]
fn list_versions_skips_blank_lines_and_malformed_json() {
    let _storage = WithStorageRoot::new();
    let path = index_file_path("my-crate").unwrap();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "\nnot json\n{\"broken\": true}\n").unwrap();

    assert!(list_versions("my-crate").is_empty());
}
